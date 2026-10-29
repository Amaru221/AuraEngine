//! Windows-specific helpers: monitors, power, fullscreen detection, registry.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicIsize, Ordering};

use anyhow::{Context, Result};
use windows::core::{w, BOOL, PCWSTR};
use windows::Win32::Foundation::{
    GetLastError, SetLastError, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, HWND, LPARAM, LRESULT, POINT,
    RECT, TRUE, WPARAM,
};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CLOAKED,
    DWMWA_COLOR_NONE, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWINDOWATTRIBUTE,
};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, MonitorFromPoint, DISPLAY_DEVICEW,
    HDC, HMONITOR, MONITORINFOEXW, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::UI::HiDpi::{
    GetDpiForMonitor, SetProcessDpiAwarenessContext, MDT_EFFECTIVE_DPI,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Shell::{
    SHQueryUserNotificationState, SetCurrentProcessExplicitAppUserModelID, QUNS_BUSY,
    QUNS_NOT_PRESENT, QUNS_RUNNING_D3D_FULL_SCREEN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, DefWindowProcW, EnumWindows, GetClassLongPtrW, GetClassNameW, GetCursorPos,
    GetForegroundWindow, GetSystemMetrics, GetWindowLongPtrW, GetWindowLongW, GetWindowRect,
    GetWindowThreadProcessId, IsIconic, FlashWindowEx, IsWindowVisible, SendMessageTimeoutW,
    SetClassLongPtrW, SetWindowLongPtrW, SetWindowPos, ShowWindow, FLASHWINFO, FLASHW_ALL,
    GCLP_HBRBACKGROUND, GCLP_WNDPROC, GWLP_WNDPROC, GWL_EXSTYLE, HWND_BROADCAST, HWND_TOP,
    SMTO_ABORTIFHUNG, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOZORDER, SWP_SHOWWINDOW,
    SW_SHOWNORMAL, WM_NCACTIVATE, WM_NCPAINT, WM_SETTINGCHANGE, WS_EX_TOOLWINDOW,
    EDD_GET_DEVICE_INTERFACE_NAME,
};

use crate::util;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct RectI {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl RectI {
    pub fn from_rect(r: RECT) -> Self {
        Self {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.bottom,
        }
    }

    pub fn width(&self) -> i32 {
        (self.right - self.left).max(0)
    }

    pub fn height(&self) -> i32 {
        (self.bottom - self.top).max(0)
    }

    pub fn area(&self) -> i64 {
        self.width() as i64 * self.height() as i64
    }

    pub fn is_empty(&self) -> bool {
        self.width() <= 0 || self.height() <= 0
    }

    pub fn intersects(&self, other: &RectI) -> i64 {
        let left = self.left.max(other.left);
        let top = self.top.max(other.top);
        let right = self.right.min(other.right);
        let bottom = self.bottom.min(other.bottom);
        let w = (right - left).max(0) as i64;
        let h = (bottom - top).max(0) as i64;
        w * h
    }
}

/// Parses the Windows display number from a monitor device name
/// (`\\.\DISPLAY2` -> `2`).
pub fn display_number(key: &str) -> Option<u32> {
    let upper = key.to_ascii_uppercase();
    let (_, digits) = upper.rsplit_once("DISPLAY")?;
    digits.parse().ok()
}

/// Normalizes a PnP monitor interface path so it can be compared across APIs.
///
/// `EnumDisplayDevicesW(..., EDD_GET_DEVICE_INTERFACE_NAME)` and
/// `IDesktopWallpaper` both report paths such as
/// `\\?\DISPLAY#GSM5C8A#5&d89a054&1&UID4353#{e6f07b5f-...}`. The trailing
/// interface GUID is the same for every monitor, so the part before it (in
/// upper case) identifies the physical panel.
pub fn normalize_monitor_identity(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let head = raw.split("#{").next().unwrap_or(raw).trim_end_matches('#');
    if head.is_empty() {
        None
    } else {
        Some(head.to_ascii_uppercase())
    }
}

/// Stable identity of the panel behind an adapter device name
/// (`\\.\DISPLAY6`). Empty when Windows cannot resolve the monitor device.
fn monitor_identity(device: &[u16]) -> String {
    let mut info = DISPLAY_DEVICEW::default();
    info.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
    let ok = unsafe {
        EnumDisplayDevicesW(
            PCWSTR(device.as_ptr()),
            0,
            &mut info,
            EDD_GET_DEVICE_INTERFACE_NAME,
        )
    }
    .as_bool();
    if !ok {
        return String::new();
    }
    normalize_monitor_identity(&util::from_wide_buf(&info.DeviceID)).unwrap_or_default()
}

#[derive(Clone, Debug)]
pub struct MonitorInfo {
    /// Device name, e.g. `\\.\DISPLAY1`. Windows renumbers it after a driver
    /// update or a port change, so it must not be used as a stable identity.
    pub key: String,
    /// Stable PnP identity of the panel (see [`normalize_monitor_identity`]).
    /// Empty when it cannot be resolved.
    pub id: String,
    /// Friendly adapter/monitor name when available.
    pub name: String,
    pub rect: RectI,
    pub work: RectI,
    pub primary: bool,
    pub dpi: u32,
}

impl MonitorInfo {
    pub fn resolution(&self) -> String {
        format!("{}x{}", self.rect.width(), self.rect.height())
    }

    /// Windows display number (1 for `\\.\DISPLAY1`), matching the numbers
    /// shown by Windows Settings -> Display -> Identify.
    pub fn number(&self) -> Option<u32> {
        display_number(&self.key)
    }

    pub fn label(&self) -> String {
        if self.name.is_empty() {
            self.key.clone()
        } else {
            self.name.clone()
        }
    }
}

static PRIMARY_INSTANCE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// True when this process created the single-instance mutex. A second launch
/// can skip the pipe probe entirely and forward its command immediately.
pub fn is_primary_instance() -> bool {
    PRIMARY_INSTANCE.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn init_process() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let _ = SetCurrentProcessExplicitAppUserModelID(w!("Aura.Engine"));

        // Named mutex that lets the installer detect a running instance
        // before upgrading or uninstalling. The handle is intentionally kept
        // for the whole process lifetime.
        let created = windows::Win32::System::Threading::CreateMutexW(
            None,
            false,
            w!("AuraEngine.SingleInstance"),
        )
        .is_ok();
        let already_running = GetLastError() == ERROR_ALREADY_EXISTS;
        PRIMARY_INSTANCE.store(
            created && !already_running,
            std::sync::atomic::Ordering::Relaxed,
        );
    }
}

/// True when `hwnd` is the current foreground window.
pub fn is_foreground(hwnd: HWND) -> bool {
    unsafe { GetForegroundWindow() == hwnd }
}

pub fn exe_path() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("aura.exe"))
}

// ---------------------------------------------------------------------------
// Monitors
// ---------------------------------------------------------------------------

struct MonitorEnumCtx {
    monitors: Vec<MonitorInfo>,
}

unsafe extern "system" fn monitor_enum_proc(
    monitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    let ctx = unsafe { &mut *(data.0 as *mut MonitorEnumCtx) };
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if unsafe { GetMonitorInfoW(monitor, &mut info as *mut _ as *mut _) }.as_bool() {
        let key = util::from_wide_buf(&info.szDevice);
        let mut device = DISPLAY_DEVICEW::default();
        device.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
        let friendly = if unsafe {
            EnumDisplayDevicesW(
                PCWSTR(info.szDevice.as_ptr()),
                0,
                &mut device,
                Default::default(),
            )
        }
        .as_bool()
        {
            util::from_wide_buf(&device.DeviceString)
        } else {
            String::new()
        };

        let mut dpi_x = 96u32;
        let mut dpi_y = 96u32;
        let _ = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };

        ctx.monitors.push(MonitorInfo {
            id: monitor_identity(&info.szDevice),
            key,
            name: friendly,
            rect: RectI::from_rect(info.monitorInfo.rcMonitor),
            work: RectI::from_rect(info.monitorInfo.rcWork),
            primary: info.monitorInfo.dwFlags & 1 != 0,
            dpi: dpi_x.max(96),
        });
    }
    TRUE
}

pub fn monitors() -> Vec<MonitorInfo> {
    let mut ctx = MonitorEnumCtx {
        monitors: Vec::new(),
    };
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(monitor_enum_proc),
            LPARAM(&mut ctx as *mut _ as isize),
        );
    }

    // A driver update can leave duplicated entries for the same panel for a
    // few enumerations. Keep the first one so the UI never draws two numbers
    // over the same screen or creates two hosts.
    let mut keys = HashSet::new();
    let mut panels = HashSet::new();
    ctx.monitors.retain(|monitor| {
        if !keys.insert(monitor.key.clone()) {
            return false;
        }
        monitor.id.is_empty() || panels.insert((monitor.id.clone(), monitor.rect))
    });

    // Deterministic order: primary first, then left to right, top to bottom.
    ctx.monitors
        .sort_by_key(|monitor| (!monitor.primary, monitor.rect.left, monitor.rect.top));
    ctx.monitors
}

pub fn virtual_screen() -> RectI {
    unsafe {
        RectI {
            left: GetSystemMetrics(SM_XVIRTUALSCREEN),
            top: GetSystemMetrics(SM_YVIRTUALSCREEN),
            right: GetSystemMetrics(SM_XVIRTUALSCREEN) + GetSystemMetrics(SM_CXVIRTUALSCREEN),
            bottom: GetSystemMetrics(SM_YVIRTUALSCREEN) + GetSystemMetrics(SM_CYVIRTUALSCREEN),
        }
    }
}

pub fn cursor_position() -> (i32, i32) {
    let mut point = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut point);
    }
    (point.x, point.y)
}

pub fn monitor_at_point(x: i32, y: i32) -> Option<MonitorInfo> {
    let _ = unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) };
    let all = monitors();
    for m in all {
        if m.rect.left <= x && x <= m.rect.right && m.rect.top <= y && y <= m.rect.bottom {
            return Some(m);
        }
    }
    None
}

pub fn monitor_for_cursor() -> Option<MonitorInfo> {
    let (x, y) = cursor_position();
    monitor_at_point(x, y)
}

// ---------------------------------------------------------------------------
// Power / fullscreen / occlusion
// ---------------------------------------------------------------------------

pub fn on_battery() -> bool {
    let mut status = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut status) }.is_err() {
        return false;
    }
    let has_battery = status.BatteryFlag != 128;
    has_battery && status.ACLineStatus == 0
}

#[allow(dead_code)]
pub fn is_fullscreen_app() -> bool {
    let monitors = monitors();
    is_fullscreen_app_on(&monitors)
}

/// Same as [`is_fullscreen_app`], reusing an existing monitor snapshot.
pub fn is_fullscreen_app_on(monitors: &[MonitorInfo]) -> bool {
    unsafe {
        let state = SHQueryUserNotificationState().unwrap_or(QUNS_NOT_PRESENT);
        if state == QUNS_RUNNING_D3D_FULL_SCREEN || state == QUNS_BUSY {
            return true;
        }

        let fg = GetForegroundWindow();
        if fg.is_invalid() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, Some(&mut pid));
        if pid == std::process::id() {
            return false;
        }
        if is_shell_window(fg) {
            return false;
        }
        let mut rect = RECT::default();
        if GetWindowRect(fg, &mut rect).is_err() {
            return false;
        }
        let rect = RectI::from_rect(rect);
        monitors
            .iter()
            .any(|m| rect.intersects(&m.rect) as f64 >= (m.rect.area() as f64) * 0.995)
    }
}

fn is_shell_window(hwnd: HWND) -> bool {
    let class = window_class(hwnd);
    matches!(
        class.as_str(),
        "Progman"
            | "WorkerW"
            | "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            | "Windows.UI.Core.CoreWindow"
            | "MultitaskingViewFrame"
            | "ForegroundStaging"
            | "XamlExplorerHostIslandWindow"
            | "TopLevelWindowForOverflowXamlIsland"
            | "NotifyIconOverflowWindow"
    )
}

fn window_class(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) };
    if len <= 0 {
        String::new()
    } else {
        util::from_wide_buf(&buf[..len as usize])
    }
}

struct OcclusionCtx {
    targets: Vec<RectI>,
    own_pid: u32,
    occluded: bool,
}

unsafe extern "system" fn occlusion_proc(hwnd: HWND, data: LPARAM) -> BOOL {
    let ctx = unsafe { &mut *(data.0 as *mut OcclusionCtx) };
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return TRUE;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == ctx.own_pid || is_shell_window(hwnd) {
            return TRUE;
        }
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if ex_style & WS_EX_TOOLWINDOW.0 != 0 {
            return TRUE;
        }

        let mut cloaked = 0u32;
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut _ as *mut _,
            std::mem::size_of::<u32>() as u32,
        );
        if cloaked != 0 {
            return TRUE;
        }

        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return TRUE;
        }
        let rect = RectI::from_rect(rect);
        if rect.is_empty() {
            return TRUE;
        }
        for target in &ctx.targets {
            let covered = rect.intersects(target);
            if covered as f64 >= (target.area() as f64) * 0.90 {
                ctx.occluded = true;
                return BOOL(0);
            }
        }
    }
    TRUE
}

/// True when a normal application window covers most of any monitor. All
/// targets are checked in a single `EnumWindows` pass.
pub fn any_occluded(monitors: &[MonitorInfo]) -> bool {
    let targets: Vec<RectI> = monitors
        .iter()
        .map(|monitor| monitor.rect)
        .filter(|rect| !rect.is_empty())
        .collect();
    if targets.is_empty() {
        return false;
    }
    let mut ctx = OcclusionCtx {
        targets,
        own_pid: std::process::id(),
        occluded: false,
    };
    unsafe {
        let _ = EnumWindows(Some(occlusion_proc), LPARAM(&mut ctx as *mut _ as isize));
    }
    ctx.occluded
}

// ---------------------------------------------------------------------------
// Personalization / registry
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub fn apps_use_light_theme() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|k| k.get_value::<u32, _>("AppsUseLightTheme"))
        .map(|v| v != 0)
        .unwrap_or(false)
}

/// Rewrites the legacy DWM accent registry values. Windows may partially ignore
/// these on recent builds, so this is opt-in and best-effort.
pub fn apply_windows_accent(r: u8, g: u8, b: u8) -> Result<()> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let abgr = 0xFF00_0000u32 | ((b as u32) << 16) | ((g as u32) << 8) | (r as u32);
    let colorization = 0xC400_0000u32 | ((b as u32) << 16) | ((g as u32) << 8) | (r as u32);

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (dwm, _) = hkcu
        .create_subkey(r"Software\Microsoft\Windows\DWM")
        .context("open DWM key")?;
    let _ = dwm.set_value("AccentColor", &abgr);
    let _ = dwm.set_value("ColorizationColor", &colorization);
    let _ = dwm.set_value("ColorizationAfterglow", &colorization);

    if let Ok((explorer_accent, _)) =
        hkcu.create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Accent")
    {
        let _ = explorer_accent.set_value("AccentColorMenu", &abgr);
    }

    unsafe {
        let param = util::to_wide("ImmersiveColorSet");
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            windows::Win32::Foundation::WPARAM(0),
            LPARAM(param.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            1000,
            None,
        );
    }
    Ok(())
}

const AUTOSTART_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const AUTOSTART_VALUE: &str = "AuraEngine";

pub fn set_autostart(enabled: bool) -> Result<()> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (run, _) = hkcu.create_subkey(AUTOSTART_KEY)?;
    if enabled {
        let exe = exe_path();
        let value = format!("\"{}\" --autostart", exe.display());
        run.set_value(AUTOSTART_VALUE, &value)?;
    } else {
        let _ = run.delete_value(AUTOSTART_VALUE);
    }
    Ok(())
}

pub fn autostart_enabled() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(AUTOSTART_KEY)
        .and_then(|k| k.get_value::<String, _>(AUTOSTART_VALUE))
        .is_ok()
}

/// Moves the main window off-screen while keeping it "visible" for winit.
///
/// Hiding it with `ShowWindow(SW_HIDE)` stops eframe from running its update
/// loop on Windows, which would freeze the engine and the quick switcher.
pub fn park_window(hwnd: HWND, hidden: bool) {
    log::info!("park_window hidden={hidden} hwnd={hwnd:?}");
    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if hidden {
            let _ = SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                ex_style | WS_EX_TOOLWINDOW.0 as isize,
            );
            let _ = SetWindowPos(
                hwnd,
                None,
                -32000,
                -32000,
                900,
                640,
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_FRAMECHANGED,
            );
        } else {
            let _ = SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                ex_style & !(WS_EX_TOOLWINDOW.0 as isize),
            );
            let monitor = monitors()
                .into_iter()
                .find(|m| m.primary)
                .map(|m| m.work)
                .unwrap_or_else(virtual_screen);
            let width = 1240.min((monitor.width() - 60).max(900));
            let height = 800.min((monitor.height() - 60).max(600));
            let x = monitor.left + (monitor.width() - width) / 2;
            let y = monitor.top + (monitor.height() - height) / 2;
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOP),
                x,
                y,
                width,
                height,
                SWP_SHOWWINDOW | SWP_FRAMECHANGED,
            );
            let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
            // Do not call SetForegroundWindow here: it can block the UI thread
            // when another application (e.g. an exclusive fullscreen game) is
            // not pumping messages. Flashing is non-blocking and enough.
            let flash = FLASHWINFO {
                cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
                hwnd,
                dwFlags: FLASHW_ALL,
                uCount: 3,
                dwTimeout: 0,
            };
            let _ = FlashWindowEx(&flash);
        }
    }
}

pub fn open_logs_folder() {
    util::open_path(&util::logs_dir());
}

/// Original winit window procedure, kept so every non-intercepted message can
/// still reach eframe/winit.
static ORIGINAL_WINDOW_PROC: AtomicIsize = AtomicIsize::new(0);

/// HWND whose window procedure has already been subclassed.
static SUBCLASSED_WINDOW: AtomicIsize = AtomicIsize::new(0);

type WndProcFn = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;

/// Suppresses the standard non-client painting of the borderless main window.
///
/// winit keeps `WS_CAPTION`/`WS_THICKFRAME` on undecorated top-level windows
/// (needed for Aero Snap and `BeginResize`) and makes the non-client area zero
/// sized from `WM_NCCALCSIZE`, but it still forwards `WM_NCACTIVATE` and
/// `WM_NCPAINT` to `DefWindowProc`. On activation (Alt+Tab, restore from
/// minimized) `DefWindowProc` paints the default non-client frame and caption
/// buttons over the UI for one or more frames, which is the white "other
/// theme" border flash. Aura draws its own title bar and border, so the
/// default paint is answered here and never reaches `DefWindowProc`.
///
/// Focus events are still delivered because winit sends `Focused` from
/// `WM_SETFOCUS`/`WM_KILLFOCUS`, not from this message.
unsafe extern "system" fn aura_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        // "The non-client area is up to date": prevents the OS frame repaint.
        WM_NCACTIVATE => return LRESULT(1),
        WM_NCPAINT => return LRESULT(0),
        _ => {}
    }

    let original = ORIGINAL_WINDOW_PROC.load(Ordering::Relaxed);
    if original != 0 {
        let proc: WndProcFn = unsafe { std::mem::transmute(original) };
        return unsafe { CallWindowProcW(Some(proc), hwnd, msg, wparam, lparam) };
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Installs [`aura_window_proc`] on the main window. Safe to call repeatedly:
/// only the first call per HWND does the subclassing.
fn install_borderless_proc(hwnd: HWND) {
    let value = hwnd.0 as isize;
    if SUBCLASSED_WINDOW.swap(value, Ordering::SeqCst) == value {
        return;
    }

    unsafe {
        SetLastError(ERROR_SUCCESS);
        let previous = SetWindowLongPtrW(
            hwnd,
            GWLP_WNDPROC,
            aura_window_proc as *const () as isize,
        );
        if GetLastError() != ERROR_SUCCESS {
            log::warn!(
                "could not subclass the main window procedure: {:?}",
                GetLastError()
            );
            SUBCLASSED_WINDOW.store(0, Ordering::SeqCst);
        } else {
            // A window can use its class procedure without having a per-window
            // one; in that case chain to the class procedure.
            let chain = if previous != 0 {
                previous
            } else {
                GetClassLongPtrW(hwnd, GCLP_WNDPROC) as isize
            };
            ORIGINAL_WINDOW_PROC.store(chain, Ordering::SeqCst);
            log::info!("borderless window procedure installed for {hwnd:?}");
        }
    }
}

/// Removes the default window-class background brush, forces DWM's dark
/// non-client rendering, drops the 1 px system border and stops Windows from
/// painting the standard frame on activation.
///
/// A freshly created borderless/transparent window can otherwise let Windows
/// paint its white default client background before the first OpenGL frame is
/// presented, which shows up as a white flash while Aura starts. Windows 11 also
/// draws a thin frame around resizable windows using the system theme color; on
/// a light theme that frame is white and stays visible for a few frames while
/// the window is revealed. Aura paints its own border in `ui::window_border`,
/// so DWM's is removed.
pub fn prepare_window(hwnd: HWND) {
    install_borderless_proc(hwnd);

    unsafe {
        let _ = SetClassLongPtrW(hwnd, GCLP_HBRBACKGROUND, 0);

        let dark = BOOL(1);
        let dark_ptr = &dark as *const _ as *const std::ffi::c_void;
        let dark_size = std::mem::size_of::<BOOL>() as u32;
        // Attribute 20 since Windows 10 20H1; older builds only know 19.
        if DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, dark_ptr, dark_size).is_err() {
            let _ = DwmSetWindowAttribute(hwnd, DWMWINDOWATTRIBUTE(19), dark_ptr, dark_size);
        }

        // Windows 11 21H2+: no system-colored frame. Older builds reject the
        // attribute, which is harmless.
        let no_border = DWMWA_COLOR_NONE;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &no_border as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        );
    }
}

/// Clips the borderless main window to rounded corners. The region is in
/// window coordinates, so it is refreshed when the window is resized or
/// maximized. Maximized windows stay square.
pub fn apply_rounded_corners(hwnd: HWND, maximized: bool) {
    let mut rect = RECT::default();
    unsafe {
        let _ = GetWindowRect(hwnd, &mut rect);
    }
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width <= 0 || height <= 0 {
        return;
    }

    static LAST_SHAPE: std::sync::Mutex<Option<(isize, i32, i32, bool)>> =
        std::sync::Mutex::new(None);
    let key = (hwnd.0 as isize, width, height, maximized);
    let mut last = LAST_SHAPE
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    if *last == Some(key) {
        return;
    }
    *last = Some(key);

    use windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, SetWindowRgn};
    unsafe {
        if maximized {
            let _ = SetWindowRgn(hwnd, None, true);
        } else {
            let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, 20, 20);
            if !region.is_invalid() {
                let _ = SetWindowRgn(hwnd, Some(region), true);
            }
        }
    }
}

#[allow(dead_code)]
pub fn reveal_path(path: &Path) {
    util::reveal_in_explorer(path);
}


#[cfg(test)]
mod tests {
    use super::{MonitorInfo, RectI};

    fn monitor(key: &str) -> MonitorInfo {
        MonitorInfo {
            key: key.to_string(),
            id: String::new(),
            name: String::new(),
            rect: RectI { left: 0, top: 0, right: 1920, bottom: 1080 },
            work: RectI { left: 0, top: 0, right: 1920, bottom: 1080 },
            primary: false,
            dpi: 96,
        }
    }

    #[test]
    fn display_numbers_match_device_names() {
        assert_eq!(monitor(r"\\.\DISPLAY1").number(), Some(1));
        assert_eq!(monitor(r"\\.\DISPLAY2").number(), Some(2));
        assert_eq!(monitor(r"\\.\DISPLAY12").number(), Some(12));
        assert_eq!(monitor("custom").number(), None);
        assert_eq!(super::display_number(r"\\.\display3"), Some(3));
        assert_eq!(super::display_number("*"), None);
    }

    #[test]
    fn monitor_identity_ignores_the_interface_guid_and_case() {
        let path = r"\\?\DISPLAY#GSM5C8A#5&d89a054&1&UID4353#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}";
        assert_eq!(
            super::normalize_monitor_identity(path).as_deref(),
            Some(r"\\?\DISPLAY#GSM5C8A#5&D89A054&1&UID4353")
        );
        // The same panel reported with another casing compares equal.
        assert_eq!(
            super::normalize_monitor_identity(
                r"\\?\display#gsm5c8a#5&d89a054&1&uid4353#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}"
            ),
            super::normalize_monitor_identity(path)
        );
        assert_eq!(super::normalize_monitor_identity(""), None);
    }
}
