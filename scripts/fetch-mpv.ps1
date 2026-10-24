#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$Destination = (Join-Path (Split-Path -Parent $PSScriptRoot) "mpv.exe"),
    [string]$WorkDir = (Join-Path $env:TEMP "aura-mpv")
)

$ErrorActionPreference = "Stop"

Write-Host "==> Querying the latest mpv-winbuild-cmake release"
$headers = @{ "User-Agent" = "AuraEngine" }
$release = Invoke-RestMethod `
    -Uri "https://api.github.com/repos/shinchiro/mpv-winbuild-cmake/releases/latest" `
    -Headers $headers

# Prefer the baseline x86_64 build; the v3 build requires a newer CPU.
$asset = $release.assets |
    Where-Object {
        $_.name -like "mpv-*.7z" -and
        $_.name -like "*x86_64*" -and
        $_.name -notlike "*dev*"
    } |
    Sort-Object @{ Expression = { $_.name -like "*v3*" } } |
    Select-Object -First 1

if (-not $asset) {
    throw "No x86_64 mpv asset found in release $($release.tag_name)"
}

$sevenZip = @(
    (Join-Path $env:ProgramFiles "7-Zip\7z.exe"),
    (Join-Path ${env:ProgramFiles(x86)} "7-Zip\7z.exe")
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1

if (-not $sevenZip) {
    throw "7z.exe not found. Install 7-Zip or use a runner that includes it."
}

New-Item -ItemType Directory -Force -Path $WorkDir | Out-Null
$archive = Join-Path $WorkDir $asset.name

if (-not (Test-Path $archive)) {
    Write-Host "==> Downloading $($asset.name)"
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $archive -Headers $headers
}

$extract = Join-Path $WorkDir "extract"
Remove-Item $extract -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $extract | Out-Null

Write-Host "==> Extracting"
& $sevenZip x $archive "-o$extract" -y | Out-Null
if ($LASTEXITCODE -ne 0) { throw "7-Zip extraction failed" }

$mpv = Get-ChildItem $extract -Recurse -Filter "mpv.exe" | Select-Object -First 1
if (-not $mpv) { throw "mpv.exe not found in the archive" }

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Destination) | Out-Null
Copy-Item $mpv.FullName $Destination -Force

Write-Host "==> mpv ready: $Destination"
Get-Item $Destination | Select-Object Name, @{ n = "MB"; e = { [math]::Round($_.Length / 1MB, 2) } }
