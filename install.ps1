#Requires -Version 5.1
[CmdletBinding()]
param(
    [switch]$NoShortcut,
    [switch]$NoStart,
    [switch]$DownloadMpv,
    [switch]$TrustCertificate
)

$ErrorActionPreference = "Stop"
$source = $PSScriptRoot
$target = Join-Path $env:LOCALAPPDATA "Aura\bin"

# Remove the Mark-of-the-Web from a downloaded ZIP so SmartScreen does not
# block the bundled binaries.
Get-ChildItem $source -Recurse -File -ErrorAction SilentlyContinue | Unblock-File -ErrorAction SilentlyContinue

Write-Host "==> Installing Aura Engine in $target"
New-Item -ItemType Directory -Force -Path $target | Out-Null

$exe = Join-Path $source "aura.exe"
if (-not (Test-Path $exe)) {
    throw "aura.exe not found next to install.ps1"
}
Copy-Item $exe $target -Force

$mpv = Join-Path $source "mpv.exe"
if (Test-Path $mpv) {
    Copy-Item $mpv $target -Force
    Write-Host "==> Bundled mpv installed"
}
elseif ($DownloadMpv) {
    Write-Host "==> Downloading portable mpv (this may take a moment)"
    $installed = Join-Path $target "aura.exe"
    Start-Process -FilePath $installed -ArgumentList "install-mpv" -NoNewWindow -Wait
}

$installedExe = Join-Path $target "aura.exe"

if (-not $NoShortcut) {
    $startMenu = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
    New-Item -ItemType Directory -Force -Path $startMenu | Out-Null
    $lnk = Join-Path $startMenu "Aura Engine.lnk"
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($lnk)
    $shortcut.TargetPath = $installedExe
    $shortcut.WorkingDirectory = $target
    $shortcut.IconLocation = "$installedExe,0"
    $shortcut.Description = "Aura Engine - Live Wallpaper Manager"
    $shortcut.Save()
    Write-Host "==> Start menu shortcut created"
}

if ($TrustCertificate) {
    $cer = Join-Path $source "certs\aura-selfsigned.cer"
    if (Test-Path $cer) {
        Write-Host "==> Trusting the Aura self-signed certificate for the current user"
        certutil -addstore -f -user Root $cer | Out-Null
        certutil -addstore -f -user TrustedPublisher $cer | Out-Null
    }
    else {
        Write-Warning "certs\aura-selfsigned.cer not found; the signature will show as untrusted."
    }
}

Write-Host "==> Installed: $installedExe"
Write-Host "    Run 'aura' from a terminal after adding $target to PATH,"
Write-Host "    or use the Start menu shortcut."

if (-not $NoStart) {
    Start-Process -FilePath $installedExe
}
