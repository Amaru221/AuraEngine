# Third-party licenses

Aura Engine itself is licensed under **GPL-3.0-or-later** (see `LICENSE` and
`NOTICE`). This file is a summary; the **full license texts** are in
[`THIRD_PARTY_LICENSES.txt`](THIRD_PARTY_LICENSES.txt).

## Aura (original project)

- Project: https://github.com/antwny/aura
- License: GPL-3.0-or-later (as declared in its `Cargo.toml`)
- Copyright: antwny and contributors
- Usage: reference implementation; the MotionBGS catalog parser and the
  palette extraction algorithm are adapted from the original source.
- Aura Engine is an unofficial port and is not affiliated with or endorsed by
  the original project.

## mpv (not bundled)

- Project: https://mpv.io / https://github.com/mpv-player/mpv
- License: GPL-2.0-or-later (some parts LGPL-2.1-or-later)
- Windows builds: https://github.com/shinchiro/mpv-winbuild-cmake
- Usage: Aura launches `mpv.exe` as a sidecar process for video playback.
  The official installer and ZIP **do not include mpv**; Aura may download a
  portable build at runtime. If mpv is downloaded, the user obtains it from
  the upstream build and its own license/source terms apply. Aura Engine does
  not redistribute mpv in its official assets.

## Rust crates and other components

Aura Engine uses open-source Rust crates (egui/eframe, windows, reqwest,
image, serde, tray-icon, global-hotkey, sevenz-rust2, ...). Their exact
versions are pinned in `Cargo.lock`. Most are dual-licensed under MIT and/or
Apache-2.0, with some under BSD/ISC/MPL/Zlib/Unicode licenses. The full
texts and component-specific license files are in
`THIRD_PARTY_LICENSES.txt`.

## Fonts

- The application loads **Segoe UI** and **Segoe MDL2 Assets** from the
  Windows installation at runtime. Those fonts are not redistributed.
- egui's default/fallback fonts are embedded by the `epaint_default_fonts`
  crate; their licenses (SIL OFL, Ubuntu Font Licence, etc.) are included in
  `THIRD_PARTY_LICENSES.txt`.

## Application icon

The Aura Engine application icon was created for this project and is
distributed under GPL-3.0-or-later together with the application.

## Online wallpaper providers

The Wallhaven, Pexels, Bing Daily and MotionBGS catalogs are accessed online.
Their content remains subject to the respective providers' terms; Aura Engine
does not redistribute those wallpapers.

## Windows API

The `windows` crate provides bindings to the Windows API and is licensed under
MIT OR Apache-2.0. Aura Engine uses the Windows SDK at build time.
