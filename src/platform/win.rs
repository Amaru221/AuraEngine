//! Windows-specific helpers: monitors, power, fullscreen detection, registry.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use windows::core::{w, BOOL, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, TRUE};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
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
    EnumWindows, GetClassNameW, GetCursorPos, GetForegroundWindow, GetSystemMetrics,
    GetWindowLongPtrW, GetWindowLongW, GetWindowRect, GetWindowThreadProcessId, IsIconic,
    FlashWindowEx, IsWindowVisible, SendMessageTimeoutW, SetWindowLongPtrW, SetWindowPos,
    ShowWindow, FLASHWINFO, FLASHW_ALL, GWL_EXSTYLE, HWND_BROADCAST, HWND_TOP, SMTO_ABORTIFHUNG,
    SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SWP_FRAMECHANGED, SWP_NOACTIVATE,
    SWP_NOZORDER, SWP_SHOWWINDOW, SW_SHOWNORMAL, WM_SETTINGCHANGE, WS_EX_TOOLWINDOW,
};

use crate::util;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
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

#[derive(Clone, Debug)]
pub struct MonitorInfo {
    /// Device name, e.g. `\\.\DISPLAY1`.
    pub key: String,
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

    pub fn label(&self) -> String {
        if self.name.is_empty() {
            self.key.clone()
        } else {
            self.name.clone()
        }
    }
}

pub fn init_process() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let _ = SetCurrentProcessExplicitAppUserModelID(w!("Aura.Engine"));

        // Named mutex that lets the installer detect a running instance
        // before upgrading or uninstalling. The handle is intentionally kept
        // for the whole process lifetime.
        let _ = windows::Win32::System::Threading::CreateMutexW(
            None,
            false,
            w!("AuraEngine.SingleInstance"),
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

pub fn is_fullscreen_app() -> bool {
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
        monitors().iter().any(|m| {
            rect.intersects(&m.rect) as f64 >= (m.rect.area() as f64) * 0.995
        })
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
    target: RectI,
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
        let covered = rect.intersects(&ctx.target);
        if covered as f64 >= (ctx.target.area() as f64) * 0.90 {
            ctx.occluded = true;
            return BOOL(0);
        }
    }
    TRUE
}

/// True when a normal application window covers most of `target`.
pub fn is_occluded(target: RectI) -> bool {
    if target.is_empty() {
        return false;
    }
    let mut ctx = OcclusionCtx {
        target,
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

/// True while Ctrl+Alt+W is physically held, used by the hold-to-show HUD.
pub fn switcher_combo_held() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_W,
    };
    unsafe {
        let down = |vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY| {
            (GetAsyncKeyState(vk.0 as i32) as u16) & 0x8000 != 0
        };
        down(VK_CONTROL) && down(VK_MENU) && down(VK_W)
    }
}

pub fn open_logs_folder() {
    util::open_path(&util::logs_dir());
}

#[allow(dead_code)]
pub fn reveal_path(path: &Path) {
    util::reveal_in_explorer(path);
}

