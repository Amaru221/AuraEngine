#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$Subject = "CN=Aura Engine (Self-Signed), O=Aura Engine, C=ES",
    [int]$Years = 3,
    [switch]$Trust
)

$ErrorActionPreference = "Stop"

Write-Host "==> Creating a self-signed code-signing certificate"
$cert = New-SelfSignedCertificate `
    -Type CodeSigningCert `
    -Subject $Subject `
    -KeyUsage DigitalSignature `
    -KeySpec Signature `
    -KeyExportPolicy NonExportable `
    -NotAfter (Get-Date).AddYears($Years) `
    -CertStoreLocation Cert:\CurrentUser\My `
    -HashAlgorithm SHA256

$certDir = Join-Path (Split-Path -Parent $PSScriptRoot) "certs"
New-Item -ItemType Directory -Force -Path $certDir | Out-Null
$cerPath = Join-Path $certDir "aura-selfsigned.cer"
Export-Certificate -Cert $cert -FilePath $cerPath -Force | Out-Null

Write-Host "Subject    : $($cert.Subject)"
Write-Host "Thumbprint : $($cert.Thumbprint)"
Write-Host "Expires    : $($cert.NotAfter)"
Write-Host "Public cert: $cerPath"

if ($Trust) {
    Write-Host "==> Trusting the certificate for the current user (Root + TrustedPublisher)"
    certutil -addstore -f -user Root $cerPath | Out-Null
    certutil -addstore -f -user TrustedPublisher $cerPath | Out-Null
    Write-Host "The certificate is now trusted on this account."
}

Write-Host ""
Write-Host "Sign with:"
Write-Host "  .\scripts\sign.ps1 -Path .\target\release\aura.exe -Thumbprint $($cert.Thumbprint)"
Write-Host ""
Write-Host "Remove it with:"
Write-Host "  Get-ChildItem Cert:\CurrentUser\My,Cert:\CurrentUser\Root,Cert:\CurrentUser\TrustedPublisher |"
Write-Host "    Where-Object { `$_.Subject -like '*Aura Engine (Self-Signed)*' } | Remove-Item"
