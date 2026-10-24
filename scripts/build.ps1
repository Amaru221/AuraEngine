#Requires -Version 5.1
[CmdletBinding()]
param(
    [switch]$SkipTests,
    [switch]$NoMpv,
    [switch]$Sign
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    if (-not $SkipTests) {
        Write-Host "==> Running tests"
        cargo test
        if ($LASTEXITCODE -ne 0) { throw "cargo test failed" }
    }

    Write-Host "==> Building release"
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

    if ($Sign) {
        Write-Host "==> Signing the executable"
        & (Join-Path $PSScriptRoot "sign.ps1") -Path "target\release\aura.exe"
        if ($LASTEXITCODE -ne 0) { throw "signing failed" }
    }

    $versionLine = Get-Content "Cargo.toml" | Where-Object { $_ -match '^version\s*=' } | Select-Object -First 1
    $version = ($versionLine -replace 'version\s*=\s*"', '' -replace '"', '').Trim()
    $name = "aura-v$version-windows-x64"
    $dist = Join-Path $root "dist\$name"

    Remove-Item $dist -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force -Path $dist | Out-Null

    Copy-Item "target\release\aura.exe" $dist
    Copy-Item "README.md", "LICENSE", "NOTICE", "THIRD_PARTY_LICENSES.md", "CHANGELOG.md", "install.ps1", "uninstall.ps1" $dist

    $publicCert = Join-Path $root "certs\aura-selfsigned.cer"
    if (-not (Test-Path $publicCert)) {
        $publicCert = Join-Path $root "dist\certs\aura-selfsigned.cer"
    }
    if (Test-Path $publicCert) {
        New-Item -ItemType Directory -Force -Path "$dist\certs" | Out-Null
        Copy-Item $publicCert "$dist\certs"
    }

    if (-not $NoMpv) {
        $mpvCandidates = @(
            (Join-Path $env:LOCALAPPDATA "Aura\bin\mpv.exe"),
            (Join-Path $root "mpv.exe")
        )
        foreach ($candidate in $mpvCandidates) {
            if (Test-Path $candidate) {
                Copy-Item $candidate $dist
                Write-Host "==> Bundled mpv: $candidate"
                break
            }
        }
    }

    $zip = "$dist.zip"
    Remove-Item $zip -Force -ErrorAction SilentlyContinue
    Compress-Archive -Path "$dist\*" -DestinationPath $zip -Force
    Write-Host "==> Package ready: $zip"
}
finally {
    Pop-Location
}
