#Requires -Version 5.1
[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [switch]$SkipTests,
    [switch]$Sign,
    [switch]$NoMpv,
    [string]$Iscc
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    if (-not $SkipBuild) {
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
    }

    if (-not $Iscc) {
        $candidates = @(
            (Join-Path $env:LOCALAPPDATA "Programs\Inno Setup 6\ISCC.exe"),
            (Join-Path ${env:ProgramFiles(x86)} "Inno Setup 6\ISCC.exe"),
            (Join-Path $env:ProgramFiles "Inno Setup 6\ISCC.exe")
        )
        $Iscc = $candidates | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
    }
    if (-not $Iscc -or -not (Test-Path $Iscc)) {
        throw "ISCC.exe not found. Install Inno Setup 6 with: winget install --id JRSoftware.InnoSetup -e"
    }

    $versionLine = Get-Content "Cargo.toml" | Where-Object { $_ -match '^version\s*=' } | Select-Object -First 1
    $version = ($versionLine -replace 'version\s*=\s*"', '' -replace '"', '').Trim()

    # mpv is optional: without it the app downloads a portable build on first use.
    $stagedMpv = Join-Path $root "installer\mpv.exe"
    if ($NoMpv) {
        Remove-Item $stagedMpv -Force -ErrorAction SilentlyContinue
    }
    else {
        $mpvCandidates = @(
            (Join-Path $root "mpv.exe"),
            (Join-Path $env:LOCALAPPDATA "Aura\bin\mpv.exe")
        )
        $mpv = $mpvCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
        if ($mpv) {
            Copy-Item $mpv $stagedMpv -Force
            Write-Host "==> Bundling mpv: $mpv"
        }
        else {
            Remove-Item $stagedMpv -Force -ErrorAction SilentlyContinue
            Write-Warning "mpv.exe not found; the installer will download it on first use."
        }
    }

    New-Item -ItemType Directory -Force -Path (Join-Path $root "dist") | Out-Null

    Write-Host "==> Compiling installer with $Iscc"
    & $Iscc "/DMyAppVersion=$version" (Join-Path $root "installer\aura.iss")
    if ($LASTEXITCODE -ne 0) { throw "ISCC failed" }

    $setup = Join-Path $root "dist\aura-v$version-windows-x64-setup.exe"
    if (-not (Test-Path $setup)) { throw "Installer not found: $setup" }

    if ($Sign) {
        Write-Host "==> Signing the installer"
        & (Join-Path $PSScriptRoot "sign.ps1") -Path $setup
        if ($LASTEXITCODE -ne 0) { throw "installer signing failed" }
    }

    $hash = (Get-FileHash $setup -Algorithm SHA256).Hash.ToLower()
    "$hash  $(Split-Path $setup -Leaf)" | Set-Content "$setup.sha256" -Encoding ASCII

    Get-Item $setup | Select-Object Name, @{ n = "MB"; e = { [math]::Round($_.Length / 1MB, 2) } }
    Write-Host "==> Installer ready: $setup"
}
finally {
    Pop-Location
}
