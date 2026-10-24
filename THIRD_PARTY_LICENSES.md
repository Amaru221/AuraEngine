# Third-party licenses

Aura Engine itself is licensed under **GPL-3.0-or-later** (see `LICENSE` and
`NOTICE`). This file lists the main third-party components.

## Aura (original project)

- Project: https://github.com/antwny/aura
- License: GPL-3.0-or-later
- Copyright: antwny and contributors
- Usage: reference implementation; the MotionBGS catalog parser and the
  palette extraction algorithm are adapted from the original source.

## mpv

- Project: https://mpv.io / https://github.com/mpv-player/mpv
- License: GPL-2.0-or-later (some parts LGPL-2.1-or-later)
- Usage: Aura launches `mpv.exe` as a sidecar process for video playback.
  The application downloads a portable build on first use if mpv is not
  installed. If you redistribute a package that includes `mpv.exe`, you must
  also provide the corresponding mpv source code (or a written offer) and its
  license text, as required by the GPL.

## Rust crates

Aura Engine uses open-source Rust crates (egui/eframe, windows, reqwest,
image, serde, tray-icon, global-hotkey, sevenz-rust2, ...). Their exact
versions are pinned in `Cargo.lock`. Most are dual-licensed under MIT and/or
Apache-2.0; the corresponding license texts are available from each crate's
repository.

## Windows API

The `windows` crate provides bindings to the Windows API and is licensed under
MIT OR Apache-2.0. Aura Engine uses the Windows SDK at build time.
