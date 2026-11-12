<#
.SYNOPSIS
  Creates the self-signed Windows code-signing certificate for release builds (ADR-0008) and stores
  it as the repository's Actions secrets.

.DESCRIPTION
  Run it yourself in a PowerShell window. It asks for the PFX password and never prints it.
    1. Creates a 3072-bit RSA code-signing certificate (SHA-256), "CN=Naval Wargame Simulator",
       valid for 3 years.
    2. Exports the private key as a password-protected PFX (keep it: it is the backup), and the
       public certificate as a .cer to install as trusted on exercise machines.
    3. Removes the key from the certificate store again (the PFX is the only copy).
    4. Sets the Actions secrets WINDOWS_CERTIFICATE (PFX, Base64) and WINDOWS_CERTIFICATE_PASSWORD
       with the GitHub CLI. Values go through a temporary file, deleted at once, so the password is
       never on a command line and gets no trailing newline.

  Needs Windows 10/11, and the GitHub CLI logged in with access to the repository (`gh auth status`).

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\signing\new-selfsigned-codesign.ps1
#>
[CmdletBinding()]
param(
  [string]$Subject = "CN=Naval Wargame Simulator",
  [int]$Years = 3,
  [string]$OutDir = (Join-Path $env:USERPROFILE "nws-signing"),
  [string]$Repo = "abeljanasa/naval-wargame-simulator",
  # Create the files only; set the secrets later.
  [switch]$SkipSecrets
)
$ErrorActionPreference = "Stop"

function Set-RepoSecret([string]$Name, [string]$Value) {
  $tmp = [System.IO.Path]::GetTempFileName()
  try {
    [System.IO.File]::WriteAllText($tmp, $Value)
    $p = Start-Process -FilePath "gh" -ArgumentList @("secret", "set", $Name, "--repo", $Repo) `
      -RedirectStandardInput $tmp -NoNewWindow -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "gh secret set $Name failed (exit $($p.ExitCode))" }
  } finally {
    Remove-Item $tmp -Force -ErrorAction SilentlyContinue
  }
}

if (-not $SkipSecrets) {
  & gh auth status *> $null
  if ($LASTEXITCODE -ne 0) { throw "The GitHub CLI is not logged in: run 'gh auth login' first, or use -SkipSecrets." }
}

$pfxPath = Join-Path $OutDir "nws-codesign.pfx"
$cerPath = Join-Path $OutDir "nws-codesign.cer"
if (Test-Path $pfxPath) { throw "$pfxPath already exists. Move it away first: it may be the only copy of the current key." }

$pw = Read-Host -AsSecureString "PFX password (12 characters or more)"
$pw2 = Read-Host -AsSecureString "Repeat the password"
$plain = [System.Net.NetworkCredential]::new("", $pw).Password
if ($plain -ne [System.Net.NetworkCredential]::new("", $pw2).Password) { throw "The passwords do not match." }
if ($plain.Length -lt 12) { throw "The password must be at least 12 characters." }

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject $Subject `
  -KeyAlgorithm RSA -KeyLength 3072 -HashAlgorithm SHA256 -KeyExportPolicy Exportable `
  -CertStoreLocation "Cert:\CurrentUser\My" -NotAfter (Get-Date).AddYears($Years)
try {
  Export-PfxCertificate -Cert $cert -FilePath $pfxPath -Password $pw | Out-Null
  Export-Certificate -Cert $cert -FilePath $cerPath | Out-Null
} finally {
  # The PFX is the copy to keep; the store holds no private key afterwards.
  Remove-Item -Path ("Cert:\CurrentUser\My\" + $cert.Thumbprint) -DeleteKey
}

if (-not $SkipSecrets) {
  Set-RepoSecret "WINDOWS_CERTIFICATE" ([Convert]::ToBase64String([System.IO.File]::ReadAllBytes($pfxPath)))
  Set-RepoSecret "WINDOWS_CERTIFICATE_PASSWORD" $plain
}
$plain = $null

Write-Output ""
Write-Output "Code-signing certificate created."
Write-Output "  Subject:     $($cert.Subject)"
Write-Output "  Thumbprint:  $($cert.Thumbprint)"
Write-Output "  Valid until: $($cert.NotAfter.ToString('yyyy-MM-dd'))"
Write-Output "  Private key: $pfxPath   (back it up somewhere safe; never commit it)"
Write-Output "  Public cert: $cerPath   (install as trusted on exercise machines)"
if ($SkipSecrets) {
  Write-Output "  Secrets NOT set (-SkipSecrets)."
} else {
  Write-Output "  Secrets set on ${Repo}: WINDOWS_CERTIFICATE, WINDOWS_CERTIFICATE_PASSWORD"
}
