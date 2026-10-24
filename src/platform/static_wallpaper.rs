//! Static wallpaper support through the `IDesktopWallpaper` COM interface.

use std::path::Path;

use anyhow::{Context, Result};
use windows::core::PCWSTR;
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL};
use windows::Win32::UI::Shell::{
    DesktopWallpaper, IDesktopWallpaper, DESKTOP_WALLPAPER_POSITION, DWPOS_CENTER, DWPOS_FILL,
    DWPOS_FIT, DWPOS_STRETCH,
};

use crate::config::Scaling;

fn position_for(scaling: Scaling) -> DESKTOP_WALLPAPER_POSITION {
    match scaling {
        Scaling::Fit => DWPOS_FIT,
        Scaling::Fill => DWPOS_FILL,
        Scaling::Stretch => DWPOS_STRETCH,
        Scaling::Center => DWPOS_CENTER,
    }
}

fn desktop() -> Result<IDesktopWallpaper> {
    unsafe { CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL) }
        .context("create IDesktopWallpaper")
}

/// Applies a static image to one monitor (by index) or to every monitor.
pub fn set_wallpaper(index: Option<usize>, path: &Path, scaling: Scaling) -> Result<()> {
    let path_str = path.to_string_lossy();
    let wide = crate::util::to_wide(&path_str);
    let dw = desktop()?;

    unsafe {
        let _ = dw.SetPosition(position_for(scaling));
    }

    let monitor_id = match index {
        Some(i) => {
            let count = unsafe { dw.GetMonitorDevicePathCount() }.unwrap_or(0);
            log::info!("static wallpaper: index {i} of {count} monitors");
            if i < count as usize {
                match unsafe { dw.GetMonitorDevicePathAt(i as u32) } {
                    Ok(path) => {
                        log::info!(
                            "  monitor device path: {}",
                            unsafe { crate::util::from_wide_ptr(path.0) }
                        );
                        Some(path)
                    }
                    Err(err) => {
                        log::error!("  GetMonitorDevicePathAt failed: {err}");
                        None
                    }
                }
            } else {
                None
            }
        }
        None => None,
    };

    let id_ptr = monitor_id
        .as_ref()
        .map(|p| PCWSTR(p.0))
        .unwrap_or_else(PCWSTR::null);

    let result = unsafe { dw.SetWallpaper(id_ptr, PCWSTR(wide.as_ptr())) };

    if let Some(p) = monitor_id {
        unsafe {
            CoTaskMemFree(Some(p.0 as *const _));
        }
    }

    result.context("SetWallpaper")?;
    Ok(())
}

/// Reads the current wallpaper for a monitor (or the desktop-wide one).
pub fn get_wallpaper(index: Option<usize>) -> Result<String> {
    let dw = desktop()?;
    let monitor_id = match index {
        Some(i) => {
            let count = unsafe { dw.GetMonitorDevicePathCount() }.unwrap_or(0);
            if i < count as usize {
                unsafe { dw.GetMonitorDevicePathAt(i as u32) }.ok()
            } else {
                None
            }
        }
        None => None,
    };
    let id_ptr = monitor_id
        .as_ref()
        .map(|p| PCWSTR(p.0))
        .unwrap_or_else(PCWSTR::null);
    let result = unsafe { dw.GetWallpaper(id_ptr) };
    if let Some(p) = monitor_id {
        unsafe {
            CoTaskMemFree(Some(p.0 as *const _));
        }
    }
    let pw = result.context("GetWallpaper")?;
    let value = unsafe { crate::util::from_wide_ptr(pw.0) };
    unsafe {
        CoTaskMemFree(Some(pw.0 as *const _));
    }
    Ok(value)
}

/// Number of monitors that Windows exposes to `IDesktopWallpaper`.
#[allow(dead_code)]
pub fn monitor_count() -> usize {
    desktop()
        .and_then(|dw| unsafe { dw.GetMonitorDevicePathCount() }.context("monitor count"))
        .map(|n| n as usize)
        .unwrap_or(0)
}

/// Snapshot of every static wallpaper, used for restoring after a live one.
#[allow(dead_code)]
pub fn snapshot() -> Vec<(usize, String)> {
    let count = monitor_count();
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        if let Ok(path) = get_wallpaper(Some(i)) {
            if !path.is_empty() {
                out.push((i, path));
            }
        }
    }
    out
}
