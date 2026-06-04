param(
  [string]$SigningKeyPath = "$env:USERPROFILE\.tauri\tokenscope-desktop.key",
  [string]$SigningKeyPassword = "",
  [switch]$SkipLatestJson
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$keyPath = [System.IO.Path]::GetFullPath($SigningKeyPath)

function Get-JsonVersion {
  param(
    [string]$Path,
    [string]$Name
  )

  if (-not (Test-Path -LiteralPath $Path)) {
    throw "Missing $Name`: $Path"
  }

  $json = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
  return [string]$json.version
}

function Assert-ReleaseVersionConsistency {
  param([string]$Root)

  $packagePath = Join-Path $Root "package.json"
  $tauriConfigPath = Join-Path $Root "src-tauri\tauri.conf.json"
  $packageVersion = Get-JsonVersion -Path $packagePath -Name "package.json"
  $tauriVersion = Get-JsonVersion -Path $tauriConfigPath -Name "tauri.conf.json"

  if ($packageVersion -ne $tauriVersion) {
    throw "Version mismatch: package.json is $packageVersion but tauri.conf.json is $tauriVersion"
  }
}

Assert-ReleaseVersionConsistency -Root $root

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
