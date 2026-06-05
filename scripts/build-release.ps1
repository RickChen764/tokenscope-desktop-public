param(
  [string]$SigningKeyPath = "$env:USERPROFILE\.tauri\tokenscope-desktop.key",
  [string]$SigningKeyPassword = "",
  [string]$NotesPath,
  [switch]$SkipLatestJson
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$script:Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
$OutputEncoding = $script:Utf8NoBom
try {
  [Console]::InputEncoding = $script:Utf8NoBom
  [Console]::OutputEncoding = $script:Utf8NoBom
} catch {
  # Non-interactive PowerShell hosts may not expose writable console encodings.
}

$root = Split-Path -Parent $PSScriptRoot
$keyPath = [System.IO.Path]::GetFullPath($SigningKeyPath)

function Read-Utf8Text {
  param([string]$Path)

  return [System.IO.File]::ReadAllText([System.IO.Path]::GetFullPath($Path), $script:Utf8NoBom)
}

function Read-JsonFile {
  param([string]$Path)

  return Read-Utf8Text -Path $Path | ConvertFrom-Json
}

function Get-JsonVersion {
  param(
    [string]$Path,
    [string]$Name
  )

  if (-not (Test-Path -LiteralPath $Path)) {
    throw "Missing $Name`: $Path"
  }

  $json = Read-JsonFile -Path $Path
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
$env:TAURI_SIGNING_PRIVATE_KEY = Read-Utf8Text -Path $keyPath
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $SigningKeyPassword

pnpm exec tauri build --ci
if ($LASTEXITCODE -ne 0) {
  throw "pnpm tauri build failed"
}

if (-not $SkipLatestJson) {
  $latestJsonArgs = @{}
  if ($NotesPath) {
    $latestJsonArgs.NotesPath = $NotesPath
  }

  & (Join-Path $PSScriptRoot "create-latest-json.ps1") @latestJsonArgs
  if ($LASTEXITCODE -ne 0) {
    throw "create-latest-json.ps1 failed"
  }
}

Write-Host "Release build finished."
Write-Host "Signing key: $keyPath"
Write-Host "Bundle dir: $(Join-Path $root 'src-tauri\target\release\bundle\nsis')"
