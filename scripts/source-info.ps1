#Requires -Version 5.1

function Write-SourceFile {
    param(
        [Parameter(Mandatory = $true)][string]$Root,
        [Parameter(Mandatory = $true)][string]$Version
    )

    $tag = "v$Version"
    $text = @"
Aura Engine $tag
Copyright (C) 2026 Amaru221 and contributors

License: GNU General Public License, version 3 or later (GPL-3.0-or-later).
See LICENSE for the full license text and NOTICE for attribution.

Corresponding source code for this release:
https://github.com/Amaru221/AuraEngine/tree/$tag

Third-party license texts:
THIRD_PARTY_LICENSES.txt

Aura Engine is an unofficial Windows port of Aura
(https://github.com/antwny/aura) and is not affiliated with or endorsed by
the original project.

mpv is NOT bundled with Aura Engine. Aura may download an mpv build at
runtime from https://github.com/shinchiro/mpv-winbuild-cmake. mpv is
licensed under GPL-2.0-or-later (some parts LGPL-2.1-or-later); its source
is available at https://github.com/mpv-player/mpv. When mpv is downloaded,
its own license and source terms apply.

This software is distributed in the hope that it will be useful, but
WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
"@

    [System.IO.File]::WriteAllText(
        (Join-Path $Root "SOURCE.txt"),
        $text,
        (New-Object System.Text.UTF8Encoding($false))
    )
}
