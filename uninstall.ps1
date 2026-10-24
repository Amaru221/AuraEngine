#Requires -Version 5.1
[CmdletBinding()]
param(
    [switch]$KeepData
)

$ErrorActionPreference = "Stop"

Write-Host "==> Stopping Aura Engine"
Get-Process aura -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-Process mpv -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -like "$env:LOCALAPPDATA\Aura\*" } |
    Stop-Process -Force -ErrorAction SilentlyContinue

$startMenu = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\Aura Engine.lnk"
if (Test-Path $startMenu) {
    Remove-Item $startMenu -Force
}

try {
    $runKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
    Remove-ItemProperty -Path $runKey -Name "AuraEngine" -ErrorAction SilentlyContinue
}
catch { }

$bin = Join-Path $env:LOCALAPPDATA "Aura"
if (Test-Path $bin) {
    Remove-Item $bin -Recurse -Force
}

if (-not $KeepData) {
    $config = Join-Path $env:APPDATA "Aura"
    if (Test-Path $config) {
        Remove-Item $config -Recurse -Force
    }
}

Write-Host "==> Removing the Aura self-signed certificate (if present)"
foreach ($store in @("Cert:\CurrentUser\My", "Cert:\CurrentUser\Root", "Cert:\CurrentUser\TrustedPublisher")) {
    Get-ChildItem $store -ErrorAction SilentlyContinue |
        Where-Object { $_.Subject -like "*Aura Engine (Self-Signed)*" } |
        Remove-Item -Force -ErrorAction SilentlyContinue
}

Write-Host "==> Aura Engine uninstalled"
