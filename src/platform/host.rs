//! Native wallpaper host windows.
//!
//! A borderless child window is created for every monitor and reparented to the
//! desktop's `WorkerW` (or `Progman` as a fallback) so that it renders *behind*
//! the desktop icons. `mpv --wid` then draws into that window.

use std::path::Path;

use anyhow::{Context, Result};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, MapWindowPoints, PAINTSTRUCT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, FindWindowExW, FindWindowW, GetClientRect,
    GetParent, GetWindowRect, IsWindow, IsWindowVisible, RegisterClassW, SendMessageTimeoutW,
    SetWindowPos, ShowWindow, HWND_BOTTOM, HTTRANSPARENT, MA_NOACTIVATE, SWP_NOACTIVATE,
    SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE, WM_ERASEBKGND, WM_MOUSEACTIVATE, WM_NCHITTEST,
    WM_PAINT, WNDCLASSW, WS_CHILD, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
    WS_VISIBLE,
};

use crate::config::ParentMode;
use crate::platform::win::{self, MonitorInfo, RectI};
use crate::util;

const WALLPAPER_CLASS: &str = "AuraEngine.WallpaperHost";

struct HostWindow {
    key: String,
    hwnd: HWND,
    rect: RectI,
    visible: bool,
}

pub struct WallpaperHosts {
    class: Vec<u16>,
    hosts: Vec<HostWindow>,
    parent: HWND,
    mode: ParentMode,
    origin: POINT,
}

impl WallpaperHosts {
    pub fn new() -> Self {
        let class = util::to_wide(WALLPAPER_CLASS);
        unsafe {
            if let Ok(instance) = GetModuleHandleW(None) {
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(wndproc),
                    hInstance: instance.into(),
                    lpszClassName: util::pcwstr(&class),
                    ..Default::default()
                };
                RegisterClassW(&wc);
            }
        }
        Self {
            class,
            hosts: Vec::new(),
            parent: HWND::default(),
            mode: ParentMode::Auto,
            origin: POINT { x: 0, y: 0 },
        }
    }

    #[allow(dead_code)]
    pub fn parent(&self) -> HWND {
        self.parent
    }

    /// Creates, moves and destroys hosts so they match `monitors`.
    ///
    /// Only monitors listed in `needed` get a host. Static wallpapers do not
    /// need one, and touching `WorkerW` for them causes Explorer to recreate
    /// the desktop layer (which used to trigger an endless re-apply loop).
    ///
    /// Returns the monitor keys whose host window had to be recreated; any
    /// wallpaper running on them must be restarted because the HWND changed.
    pub fn sync(
        &mut self,
        monitors: &[MonitorInfo],
        needed: &[String],
        mode: ParentMode,
    ) -> Vec<String> {
        let mut changed = Vec::new();

        if needed.is_empty() {
            if !self.hosts.is_empty() {
                changed.extend(self.hosts.iter().map(|h| h.key.clone()));
                self.destroy_all();
            }
            return changed;
        }

        let parent_dead = self.parent.is_invalid() || !unsafe { IsWindow(Some(self.parent)).as_bool() };
        if parent_dead || mode != self.mode {
            self.parent = find_parent(mode);
            self.mode = mode;
            log::info!(
                "wallpaper parent: {:?} ({:?})",
                self.parent,
                parent_label(self.parent)
            );
            changed.extend(self.hosts.iter().map(|h| h.key.clone()));
            // Children of the previous parent must be recreated.
            self.destroy_all();
        }

        // Explorer restarted or the window was destroyed: rebuild everything.
        for host in &self.hosts {
            let parent_ok = unsafe { GetParent(host.hwnd) }
                .map(|p| p == self.parent)
                .unwrap_or(false);
            if !unsafe { IsWindow(Some(host.hwnd)).as_bool() } || !parent_ok {
                changed.extend(self.hosts.iter().map(|h| h.key.clone()));
                self.destroy_all();
                break;
            }
        }

        let origin = parent_client_origin(self.parent);
        let reposition = origin != self.origin;
        self.origin = origin;
        let mut next: Vec<HostWindow> = Vec::with_capacity(monitors.len());

        for monitor in monitors
            .iter()
            .filter(|monitor| needed.iter().any(|key| key == &monitor.key))
        {
            let existing = self
                .hosts
                .iter()
                .position(|h| h.key == monitor.key && h.rect == monitor.rect && !changed.contains(&h.key));

            if let Some(index) = existing {
                let host = self.hosts.remove(index);
                if reposition {
                    let x = monitor.rect.left - origin.x;
                    let y = monitor.rect.top - origin.y;
                    unsafe {
                        let _ = SetWindowPos(
                            host.hwnd,
                            Some(HWND_BOTTOM),
                            x,
                            y,
                            monitor.rect.width(),
                            monitor.rect.height(),
                            SWP_NOACTIVATE | SWP_SHOWWINDOW,
                        );
                    }
                }
                next.push(host);
                continue;
            }

            // Remove a stale host for this monitor, then create a fresh one.
            if let Some(index) = self.hosts.iter().position(|h| h.key == monitor.key) {
                let stale = self.hosts.remove(index);
                unsafe {
                    let _ = DestroyWindow(stale.hwnd);
                }
                changed.push(monitor.key.clone());
            }

            match create_host(
                &self.class,
                self.parent,
                monitor,
                origin,
                true,
            ) {
                Ok(hwnd) => {
                    log::info!(
                        "host created for {} at {}x{} (hwnd {:?})",
                        monitor.key,
                        monitor.rect.width(),
                        monitor.rect.height(),
                        hwnd
                    );
                    next.push(HostWindow {
                        key: monitor.key.clone(),
                        hwnd,
                        rect: monitor.rect,
                        visible: true,
                    });
                    changed.push(monitor.key.clone());
                }
                Err(err) => {
                    log::error!("could not create host for {}: {err:#}", monitor.key);
                }
            }
        }

        // Destroy hosts whose monitor disappeared.
        for host in self.hosts.drain(..) {
            unsafe {
                let _ = DestroyWindow(host.hwnd);
            }
            changed.push(host.key);
        }

        self.hosts = next;
        changed.sort();
        changed.dedup();
        changed
    }

    /// Creates a host for a single monitor without touching the others.
    /// Used when a video is applied before the periodic sync runs.
    pub fn ensure(&mut self, monitor: &MonitorInfo, mode: ParentMode) -> Option<HWND> {
        if let Some(hwnd) = self.hwnd_for(&monitor.key) {
            return Some(hwnd);
        }

        let parent_dead =
            self.parent.is_invalid() || !unsafe { IsWindow(Some(self.parent)).as_bool() };
        if parent_dead || mode != self.mode {
            self.parent = find_parent(mode);
            self.mode = mode;
            self.destroy_all();
        }

        let origin = parent_client_origin(self.parent);
        self.origin = origin;
        match create_host(&self.class, self.parent, monitor, origin, true) {
            Ok(hwnd) => {
                log::info!("host created on demand for {} (hwnd {:?})", monitor.key, hwnd);
                self.hosts.push(HostWindow {
                    key: monitor.key.clone(),
                    hwnd,
                    rect: monitor.rect,
                    visible: true,
                });
                Some(hwnd)
            }
            Err(err) => {
                log::error!("could not create host for {}: {err:#}", monitor.key);
                None
            }
        }
    }

    pub fn hwnd_for(&self, key: &str) -> Option<HWND> {
        self.hosts
            .iter()
            .find(|h| h.key == key)
            .map(|h| h.hwnd)
            .filter(|h| unsafe { IsWindow(Some(*h)).as_bool() })
    }

    #[allow(dead_code)]
    pub fn primary_hwnd(&self, monitors: &[MonitorInfo]) -> Option<(String, HWND)> {
        monitors
            .iter()
            .find(|m| m.primary)
            .or_else(|| monitors.first())
            .and_then(|m| self.hwnd_for(&m.key).map(|h| (m.key.clone(), h)))
    }

    pub fn set_visible(&mut self, key: &str, visible: bool) {
        if let Some(host) = self.hosts.iter_mut().find(|h| h.key == key) {
            unsafe {
                let _ = ShowWindow(
                    host.hwnd,
                    if visible { SW_SHOWNOACTIVATE } else { SW_HIDE },
                );
            }
            host.visible = visible;
        }
    }

    pub fn show_all(&mut self, visible: bool) {
        for host in &mut self.hosts {
            unsafe {
                let _ = ShowWindow(
                    host.hwnd,
                    if visible { SW_SHOWNOACTIVATE } else { SW_HIDE },
                );
            }
            host.visible = visible;
        }
    }

    pub fn destroy_all(&mut self) {
        for host in self.hosts.drain(..) {
            unsafe {
                let _ = DestroyWindow(host.hwnd);
            }
        }
    }

    #[allow(dead_code)]
    pub fn keys(&self) -> Vec<String> {
        self.hosts.iter().map(|h| h.key.clone()).collect()
    }
}

impl Drop for WallpaperHosts {
    fn drop(&mut self) {
        self.destroy_all();
    }
}

fn create_host(
    class: &[u16],
    parent: HWND,
    monitor: &MonitorInfo,
    origin: POINT,
    visible: bool,
) -> Result<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None).context("GetModuleHandleW")?;
        let x = monitor.rect.left - origin.x;
        let y = monitor.rect.top - origin.y;
        let hwnd = CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT,
            util::pcwstr(class),
            w!("Aura Wallpaper"),
            WS_CHILD | WS_VISIBLE,
            x,
            y,
            monitor.rect.width(),
            monitor.rect.height(),
            Some(parent),
            None,
            Some(instance.into()),
            None,
        )
        .context("CreateWindowExW")?;

        let _ = SetWindowPos(
            hwnd,
            Some(HWND_BOTTOM),
            x,
            y,
            monitor.rect.width(),
            monitor.rect.height(),
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        if !visible {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        Ok(hwnd)
    }
}

fn parent_client_origin(parent: HWND) -> POINT {
    if parent.is_invalid() {
        return POINT { x: 0, y: 0 };
    }
    let mut rect = RECT::default();
    unsafe {
        let _ = GetClientRect(parent, &mut rect);
    }
    // WorkerW covers the virtual desktop, so its client origin is normally the
    // up-left corner of the whole desktop. Map it to screen coordinates.
    let mut points = [POINT { x: 0, y: 0 }];
    unsafe {
        MapWindowPoints(Some(parent), None, &mut points);
    }
    points[0]
}

fn parent_label(hwnd: HWND) -> String {
    if hwnd.is_invalid() {
        return "none".into();
    }
    unsafe {
        let mut name = [0u16; 128];
        let copied = windows::Win32::UI::WindowsAndMessaging::GetClassNameW(hwnd, &mut name);
        if copied > 0 {
            util::from_wide_buf(&name[..copied as usize])
        } else {
            format!("{hwnd:?}")
        }
    }
}

/// Locates the desktop layer that sits behind the icons.
pub fn find_parent(mode: ParentMode) -> HWND {
    unsafe {
        let progman = FindWindowW(w!("Progman"), PCWSTR::null()).unwrap_or_default();
        if mode == ParentMode::Progman && !progman.is_invalid() {
            return progman;
        }

        if !progman.is_invalid() {
            // Ask Explorer to spawn the wallpaper WorkerW. Different Windows
            // builds respond to different parameter combinations.
            let mut result = 0usize;
            for (wparam, lparam) in [(0xDusize, 1isize), (0xD, 0), (0, 0)] {
                SendMessageTimeoutW(
                    progman,
                    0x052C,
                    WPARAM(wparam),
                    LPARAM(lparam),
                    windows::Win32::UI::WindowsAndMessaging::SMTO_NORMAL,
                    250,
                    Some(&mut result),
                );
            }
        }

        let virtual_rect = win::virtual_screen();

        // Windows 11 (and 24H2+) creates the wallpaper surface as a WorkerW
        // *child of Progman* after the 0x052C message.
        if !progman.is_invalid() {
            let mut child = HWND::default();
            loop {
                let next = FindWindowExW(
                    Some(progman),
                    if child.is_invalid() { None } else { Some(child) },
                    w!("WorkerW"),
                    PCWSTR::null(),
                );
                let Ok(candidate) = next else {
                    break;
                };
                child = candidate;
                let mut rect = RECT::default();
                if GetWindowRect(candidate, &mut rect).is_ok() {
                    let r = RectI::from_rect(rect);
                    let large_enough = r.width() as i64 * r.height() as i64
                        >= (virtual_rect.area() / 2).max(1);
                    if large_enough {
                        log::info!("using Windows 11 child WorkerW {:?}", candidate);
                        return candidate;
                    }
                }
            }
        }

        let mut workers: Vec<HWND> = Vec::new();
        let mut current = HWND::default();
        loop {
            match FindWindowExW(
                None,
                if current.is_invalid() {
                    None
                } else {
                    Some(current)
                },
                w!("WorkerW"),
                PCWSTR::null(),
            ) {
                Ok(next) => {
                    current = next;
                    workers.push(next);
                }
                Err(_) => break,
            }
            if workers.len() > 64 {
                break;
            }
        }

        let mut defview_worker = HWND::default();
        for worker in &workers {
            if FindWindowExW(
                Some(*worker),
                None,
                w!("SHELLDLL_DefView"),
                PCWSTR::null(),
            )
            .is_ok()
            {
                defview_worker = *worker;
                break;
            }
        }

        let mut candidate = HWND::default();
        if !defview_worker.is_invalid() {
            // Classic Windows 10 layout: the WorkerW right after the one that
            // hosts the icon view is the wallpaper layer.
            if let Ok(next) = FindWindowExW(None, Some(defview_worker), w!("WorkerW"), PCWSTR::null())
            {
                candidate = next;
            }
        }

        if candidate.is_invalid() {
            // Windows 11 layout (and 24H2+): pick a WorkerW that covers the
            // virtual desktop but does not contain the icon view.
            for worker in &workers {
                if *worker == defview_worker {
                    continue;
                }
                let mut rect = RECT::default();
                if GetWindowRect(*worker, &mut rect).is_err() {
                    continue;
                }
                let r = RectI::from_rect(rect);
                let covers = r.left <= virtual_rect.left + 2
                    && r.top <= virtual_rect.top + 2
                    && r.right >= virtual_rect.right - 2
                    && r.bottom >= virtual_rect.bottom - 2;
                if covers || workers.len() == 1 {
                    candidate = *worker;
                    break;
                }
            }
        }

        if candidate.is_invalid() {
            if mode == ParentMode::WorkerW {
                log::warn!("WorkerW not found; falling back to Progman");
            }
            candidate = progman;
        }
        candidate
    }
}

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            unsafe {
                let _ = BeginPaint(hwnd, &mut paint);
                let _ = EndPaint(hwnd, &paint);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Convenience used by the diagnostics command.
pub fn describe_parent(mode: ParentMode) -> (String, bool) {
    let hwnd = find_parent(mode);
    let mut rect = RECT::default();
    let ok = unsafe { GetWindowRect(hwnd, &mut rect).is_ok() };
    (parent_label(hwnd), ok)
}

/// Human-readable map of the desktop layers, for `aura doctor`.
pub fn describe_desktop() -> String {
    let mut out = String::new();
    unsafe {
        let progman = FindWindowW(w!("Progman"), PCWSTR::null()).unwrap_or_default();
        out.push_str(&format!("Progman: {:?}\n", progman));
        if !progman.is_invalid() {
            let mut rect = RECT::default();
            let _ = GetWindowRect(progman, &mut rect);
            out.push_str(&format!(
                "  rect: {},{}-{},{} visible={}\n",
                rect.left,
                rect.top,
                rect.right,
                rect.bottom,
                IsWindowVisible(progman).as_bool()
            ));

            let mut child = HWND::default();
            loop {
                let next = FindWindowExW(
                    Some(progman),
                    if child.is_invalid() { None } else { Some(child) },
                    w!("WorkerW"),
                    PCWSTR::null(),
                );
                let Ok(candidate) = next else {
                    break;
                };
                child = candidate;
                let mut r = RECT::default();
                let _ = GetWindowRect(candidate, &mut r);
                out.push_str(&format!(
                    "  child WorkerW {:?} visible={} rect: {},{}-{},{}\n",
                    candidate,
                    IsWindowVisible(candidate).as_bool(),
                    r.left,
                    r.top,
                    r.right,
                    r.bottom
                ));
            }
        }

        let mut count = 0;
        let mut current = HWND::default();
        loop {
            let next = FindWindowExW(
                None,
                if current.is_invalid() { None } else { Some(current) },
                w!("WorkerW"),
                PCWSTR::null(),
            );
            let Ok(candidate) = next else {
                break;
            };
            current = candidate;
            count += 1;
        }
        out.push_str(&format!("top-level WorkerW windows: {count}\n"));

        let chosen = find_parent(ParentMode::Auto);
        let mut rect = RECT::default();
        let _ = GetWindowRect(chosen, &mut rect);
        out.push_str(&format!(
            "chosen parent: {:?} ({}) rect: {},{}-{},{}\n",
            chosen,
            parent_label(chosen),
            rect.left,
            rect.top,
            rect.right,
            rect.bottom
        ));

        // Probe a real host window so parenting can be verified.
        let monitors = win::monitors();
        if let Some(monitor) = monitors.first() {
            let mut hosts = WallpaperHosts::new();
            let _ = hosts.sync(
                std::slice::from_ref(monitor),
                &[monitor.key.clone()],
                ParentMode::Auto,
            );
            if let Some(hwnd) = hosts.hwnd_for(&monitor.key) {
                let parent = GetParent(hwnd).unwrap_or_default();
                let mut hr = RECT::default();
                let _ = GetWindowRect(hwnd, &mut hr);
                out.push_str(&format!(
                    "probe host: {:?} parent={:?} visible={} rect: {},{}-{},{}\n",
                    hwnd,
                    parent,
                    IsWindowVisible(hwnd).as_bool(),
                    hr.left,
                    hr.top,
                    hr.right,
                    hr.bottom
                ));
            } else {
                out.push_str("probe host: could not be created\n");
            }
        }
    }
    out
}

#[allow(dead_code)]
pub fn ensure_path(_path: &Path) {}
