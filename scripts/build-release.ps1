param(
  [string]$SigningKeyPath = "$env:USERPROFILE\.tauri\tokenscope-desktop.key",
  [string]$SigningKeyPassword = "",
  [switch]$SkipLatestJson
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$keyPath = [System.IO.Path]::GetFullPath($SigningKeyPath)

if (-not (Test-Path -LiteralPath $keyPath)) {
  throw "Missing Tauri updater signing key: $keyPath"
}

$env:TAURI_SIGNING_PRIVATE_KEY_PATH = $keyPath
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -LiteralPath $keyPath -Raw
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $SigningKeyPassword

pnpm exec tauri build --ci
if ($LASTEXITCODE -ne 0) {
  throw "pnpm tauri build failed"
}

if (-not $SkipLatestJson) {
  & (Join-Path $PSScriptRoot "create-latest-json.ps1")
  if ($LASTEXITCODE -ne 0) {
    throw "create-latest-json.ps1 failed"
  }
}

Write-Host "Release build finished."
Write-Host "Signing key: $keyPath"
Write-Host "Bundle dir: $(Join-Path $root 'src-tauri\target\release\bundle\nsis')"
