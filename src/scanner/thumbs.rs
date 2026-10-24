//! Thumbnail generation with an mpv frame extractor and a Shell fallback.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};

use crate::config::MediaKind;
use crate::util;

#[derive(Clone, Debug)]
pub struct RawImage {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Debug)]
pub enum ThumbRequest {
    Local {
        key: String,
        path: PathBuf,
        kind: MediaKind,
        mpv: Option<PathBuf>,
    },
    Remote {
        key: String,
        url: String,
    },
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct ThumbResult {
    pub key: String,
    pub image: Option<RawImage>,
    pub error: Option<String>,
}

const THUMB_W: u32 = 480;
const THUMB_H: u32 = 270;

pub fn spawn_worker() -> (Sender<ThumbRequest>, Receiver<ThumbResult>) {
    let (tx_req, rx_req) = crossbeam_channel::unbounded::<ThumbRequest>();
    let (tx_res, rx_res) = crossbeam_channel::unbounded::<ThumbResult>();

    std::thread::Builder::new()
        .name("aura-thumbs".into())
        .spawn(move || {
            unsafe {
                let _ = windows::Win32::System::Com::CoInitializeEx(
                    None,
                    windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
                );
            }
            while let Ok(request) = rx_req.recv() {
                let result = match request {
                    ThumbRequest::Local {
                        key,
                        path,
                        kind,
                        mpv,
                    } => generate_local(&key, &path, kind, mpv.as_deref()),
                    ThumbRequest::Remote { key, url } => generate_remote(&key, &url),
                };
                if tx_res.send(result).is_err() {
                    break;
                }
            }
        })
        .ok();

    (tx_req, rx_res)
}

fn cache_path(key: &str) -> PathBuf {
    util::thumbs_dir().join(format!("{key}.jpg"))
}

fn generate_local(
    key: &str,
    path: &Path,
    kind: MediaKind,
    mpv: Option<&Path>,
) -> ThumbResult {
    let cached = cache_path(key);
    if cached.is_file() {
        if let Ok(image) = image::open(&cached) {
            return ok(key, image);
        }
    }

    let _ = std::fs::create_dir_all(util::thumbs_dir());
    let temp_png = util::thumbs_dir().join(format!("{key}-frame.png"));

    let loaded = match kind {
        MediaKind::Image => image::open(path).ok(),
        MediaKind::Video => {
            let mut frame = None;
            if let Some(mpv) = mpv {
                for at in [1u64, 0] {
                    if crate::engine::mpv::extract_frame(mpv, path, &temp_png, at) {
                        frame = image::open(&temp_png).ok();
                        break;
                    }
                }
            }
            if frame.is_none() {
                frame = shell_thumbnail(path);
            }
            frame
        }
    };

    let _ = std::fs::remove_file(&temp_png);

    match loaded {
        Some(image) => {
            let thumb = image.thumbnail(THUMB_W, THUMB_H);
            let _ = thumb.save_with_format(&cached, image::ImageFormat::Jpeg);
            ok(key, thumb)
        }
        None => ThumbResult {
            key: key.to_string(),
            image: None,
            error: Some("could not generate thumbnail".into()),
        },
    }
}

fn generate_remote(key: &str, url: &str) -> ThumbResult {
    let cached = cache_path(key);
    if cached.is_file() {
        if let Ok(image) = image::open(&cached) {
            return ok(key, image);
        }
    }
    let _ = std::fs::create_dir_all(util::thumbs_dir());

    let bytes = reqwest::blocking::Client::builder()
        .user_agent("AuraEngine/1.0")
        .timeout(Duration::from_secs(30))
        .build()
        .ok()
        .and_then(|client| client.get(url).send().ok())
        .and_then(|resp| resp.error_for_status().ok())
        .and_then(|resp| resp.bytes().ok());

    let Some(bytes) = bytes else {
        return ThumbResult {
            key: key.to_string(),
            image: None,
            error: Some("thumbnail download failed".into()),
        };
    };

    match image::load_from_memory(&bytes) {
        Ok(image) => {
            let thumb = image.thumbnail(THUMB_W, THUMB_H);
            let _ = thumb.save_with_format(&cached, image::ImageFormat::Jpeg);
            ok(key, thumb)
        }
        Err(err) => ThumbResult {
            key: key.to_string(),
            image: None,
            error: Some(format!("decode thumbnail: {err}")),
        },
    }
}

fn ok(key: &str, image: image::DynamicImage) -> ThumbResult {
    let rgba = image.to_rgba8();
    ThumbResult {
        key: key.to_string(),
        image: Some(RawImage {
            width: rgba.width() as usize,
            height: rgba.height() as usize,
            rgba: rgba.into_raw(),
        }),
        error: None,
    }
}

// ---------------------------------------------------------------------------
// Windows Shell thumbnail fallback
// ---------------------------------------------------------------------------

fn shell_thumbnail(path: &Path) -> Option<image::DynamicImage> {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK,
    };

    unsafe {
        let factory: IShellItemImageFactory =
            SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None).ok()?;
        let bitmap = factory
            .GetImage(
                SIZE {
                    cx: THUMB_W as i32,
                    cy: THUMB_H as i32,
                },
                SIIGBF_BIGGERSIZEOK,
            )
            .ok()?;
        let raw = hbitmap_to_rgba(bitmap)?;
        image::RgbaImage::from_raw(raw.width as u32, raw.height as u32, raw.rgba)
            .map(image::DynamicImage::ImageRgba8)
    }
}

fn hbitmap_to_rgba(
    bitmap: windows::Win32::Graphics::Gdi::HBITMAP,
) -> Option<RawImage> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, GetDC, GetDIBits, GetObjectW,
        HGDIOBJ, ReleaseDC,
    };

    unsafe {
        let mut bmp = BITMAP::default();
        let got = GetObjectW(
            HGDIOBJ(bitmap.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bmp as *mut _ as *mut _),
        );
        if got == 0 || bmp.bmWidth <= 0 || bmp.bmHeight <= 0 {
            return None;
        }
        let width = bmp.bmWidth as usize;
        let height = bmp.bmHeight as usize;

        let mut info = BITMAPINFO::default();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: bmp.bmWidth,
            biHeight: -bmp.bmHeight,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };

        let mut pixels = vec![0u8; width * height * 4];
        let hdc = GetDC(None);
        let lines = GetDIBits(
            hdc,
            bitmap,
            0,
            bmp.bmHeight as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut info,
            DIB_RGB_COLORS,
        );
        let _ = ReleaseDC(Some(HWND::default()), hdc);
        if lines == 0 {
            return None;
        }

        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            if pixel[3] == 0 {
                pixel[3] = 255;
            }
        }

        Some(RawImage {
            width,
            height,
            rgba: pixels,
        })
    }
}
