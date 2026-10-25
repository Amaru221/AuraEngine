#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$Output
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not $Output) {
    $Output = Join-Path $root "THIRD_PARTY_LICENSES.txt"
}
Push-Location $root
try {
    $metadata = cargo metadata --format-version 1 | ConvertFrom-Json
    $lines = New-Object System.Collections.Generic.List[string]

    $lines.Add("Aura Engine - Third-party licenses")
    $lines.Add("================================================================================")
    $lines.Add("")
    $lines.Add("Aura Engine itself is licensed under the GNU General Public License,")
    $lines.Add("version 3 or any later version (GPL-3.0-or-later). See LICENSE and NOTICE.")
    $lines.Add("")
    $lines.Add("Original Aura project")
    $lines.Add("  Project:   https://github.com/antwny/aura")
    $lines.Add("  License:   GPL-3.0-or-later (as declared in its Cargo.toml)")
    $lines.Add("  Copyright: antwny and contributors")
    $lines.Add("  Aura Engine is an unofficial Windows port and is not affiliated with")
    $lines.Add("  or endorsed by the original Aura project.")
    $lines.Add("")
    $lines.Add("mpv (NOT bundled with Aura Engine)")
    $lines.Add("  Aura can download an mpv build at runtime from")
    $lines.Add("  https://github.com/shinchiro/mpv-winbuild-cmake.")
    $lines.Add("  mpv is licensed under GPL-2.0-or-later (some parts LGPL-2.1-or-later).")
    $lines.Add("  Source: https://github.com/mpv-player/mpv")
    $lines.Add("  When mpv is downloaded, its own license and source terms apply.")
    $lines.Add("")
    $lines.Add("Windows system fonts")
    $lines.Add("  Aura loads Segoe UI and Segoe MDL2 Assets from the Windows installation at")
    $lines.Add("  runtime. Those fonts are not redistributed with Aura Engine.")
    $lines.Add("")
    $lines.Add("Application icon")
    $lines.Add("  The Aura Engine application icon was created for this project and is")
    $lines.Add("  distributed under GPL-3.0-or-later together with the application.")
    $lines.Add("")
    $lines.Add("Online wallpaper providers")
    $lines.Add("  The Wallhaven, Pexels, Bing Daily and MotionBGS catalogs are accessed")
    $lines.Add("  online. Their content remains subject to the respective providers' terms;")
    $lines.Add("  Aura Engine does not redistribute those wallpapers.")
    $lines.Add("")
    $lines.Add("Rust crates and other components")
    $lines.Add("================================================================================")
    foreach ($package in ($metadata.packages | Where-Object { $_.name -ne "aura" } | Sort-Object name, version)) {
        $license = if ($package.license) { $package.license } else { "(not declared)" }
        $lines.Add("")
        $lines.Add("$($package.name) $($package.version)")
        $lines.Add("  License: $license")
        if ($package.authors) {
            $lines.Add("  Authors: $($package.authors -join ', ')")
        }
        if ($package.repository) {
            $lines.Add("  Repository: $($package.repository)")
        }
    }

    $lines.Add("")
    $lines.Add("================================================================================")
    $lines.Add("Standard MIT license text")
    $lines.Add("================================================================================")
    $lines.Add("")
    $lines.Add((Get-Content (Join-Path $root "licenses\MIT.txt") -Raw))

    $lines.Add("")
    $lines.Add("================================================================================")
    $lines.Add("Apache License, Version 2.0")
    $lines.Add("================================================================================")
    $lines.Add("")
    $lines.Add((Get-Content (Join-Path $root "licenses\Apache-2.0.txt") -Raw))

    $lines.Add("")
    $lines.Add("================================================================================")
    $lines.Add("Component-specific license files")
    $lines.Add("================================================================================")

    foreach ($package in ($metadata.packages | Where-Object { $_.name -ne "aura" } | Sort-Object name, version)) {
        $directory = Split-Path -Parent $package.manifest_path
        $files = @(Get-ChildItem -Path $directory -Recurse -File -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -match '(?i)(licen[cs]e|copying|notice|ofl|ufl)' } |
            Sort-Object FullName)
        foreach ($file in $files) {
            $relative = $file.FullName.Substring($directory.Length).TrimStart('\', '/')
            $lines.Add("")
            $lines.Add("--- $($package.name) $($package.version): $relative ---")
            $lines.Add((Get-Content $file.FullName -Raw))
        }
    }

    $lines.Add("")
    [System.IO.File]::WriteAllLines($Output, $lines, (New-Object System.Text.UTF8Encoding($false)))
    Write-Host "Generated $Output ($((Get-Item $Output).Length) bytes)"
}
finally {
    Pop-Location
}
