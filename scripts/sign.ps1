#Requires -Version 5.1
[CmdletBinding(DefaultParameterSetName = "Thumbprint")]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string[]]$Path,

    [Parameter(ParameterSetName = "Pfx")]
    [string]$Pfx,

    [Parameter(ParameterSetName = "Pfx")]
    [string]$PfxPassword,

    [Parameter(ParameterSetName = "Thumbprint")]
    [string]$Thumbprint,

    [string]$TimestampUrl = "http://timestamp.digicert.com"
)

$ErrorActionPreference = "Stop"

$signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\' } |
    Sort-Object FullName -Descending |
    Select-Object -First 1 -ExpandProperty FullName

if (-not $signtool) {
    throw "signtool.exe not found. Install the Windows SDK (Desktop C++ workload)."
}

if (-not $Pfx -and -not $Thumbprint) {
    $cert = Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert |
        Where-Object HasPrivateKey |
        Sort-Object @{ Expression = { if ($_.Subject -like "*Aura Engine*") { 0 } else { 1 } } } |
        Select-Object -First 1
    if (-not $cert) {
        throw "No code-signing certificate found. Run scripts\create-selfsigned-cert.ps1 or provide -Pfx/-Thumbprint."
    }
    $Thumbprint = $cert.Thumbprint
    Write-Host "Using certificate: $($cert.Subject) ($Thumbprint)"
}

foreach ($file in $Path) {
    if (-not (Test-Path $file)) {
        throw "File not found: $file"
    }

    Write-Host "==> Signing $file"
    if ($Pfx) {
        & $signtool sign /f $Pfx /p $PfxPassword /fd SHA256 /tr $TimestampUrl /td SHA256 /v $file
    }
    else {
        & $signtool sign /sha1 $Thumbprint /fd SHA256 /tr $TimestampUrl /td SHA256 /v $file
    }
    if ($LASTEXITCODE -ne 0) {
        throw "Signing failed for $file"
    }

    Write-Host "==> Verifying $file"
    & $signtool verify /pa /v $file
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "The signature is present but the chain is not trusted on this machine. Import the .cer into Trusted Root and Trusted Publisher, or use a real CA certificate for distribution."
    }
}

Write-Host "Done."
