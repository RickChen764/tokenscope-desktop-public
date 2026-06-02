param(
  [string]$RepoFullName = "RickChen764/tokenscope-desktop-public",
  [string]$Version,
  [string]$Notes = "TokenScope Desktop updater package.",
  [string]$BundleDir,
  [string]$OutputPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-ProjectRoot {
  return [System.IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
}

function Get-TauriVersion {
  param([string]$Root)

  $configPath = Join-Path $Root "src-tauri\tauri.conf.json"
  $config = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
  return [string]$config.version
}

function ConvertTo-GithubAssetName {
  param([string]$FileName)

  return $FileName -replace "\s+", "."
}

$root = Get-ProjectRoot
if (-not $Version) {
  $Version = Get-TauriVersion -Root $root
}

if (-not $BundleDir) {
  $BundleDir = Join-Path $root "src-tauri\target\release\bundle\nsis"
}
$BundleDir = [System.IO.Path]::GetFullPath($BundleDir)

if (-not $OutputPath) {
  $OutputPath = Join-Path $BundleDir "latest.json"
}

$installerPattern = "*_$($Version)_x64-setup.exe"
$updateBundle = Get-ChildItem -LiteralPath $BundleDir -Filter $installerPattern |
  Sort-Object LastWriteTime -Descending |
  Select-Object -First 1

if (-not $updateBundle) {
  throw "No NSIS installer found in $BundleDir for version $Version"
}

$signaturePath = "$($updateBundle.FullName).sig"
if (-not (Test-Path -LiteralPath $signaturePath)) {
  throw "Missing updater signature: $signaturePath"
}

$assetName = ConvertTo-GithubAssetName -FileName $updateBundle.Name
$publishedBundlePath = Join-Path $BundleDir $assetName
$publishedSignaturePath = "$publishedBundlePath.sig"

if ($updateBundle.FullName -ne $publishedBundlePath) {
  Copy-Item -LiteralPath $updateBundle.FullName -Destination $publishedBundlePath -Force
  Copy-Item -LiteralPath $signaturePath -Destination $publishedSignaturePath -Force
}

$urlVersion = if ($Version.StartsWith("v")) { $Version } else { "v$Version" }
$signature = (Get-Content -LiteralPath $signaturePath -Raw).Trim()

$latestJson = [ordered]@{
  version = $Version
  notes = $Notes
  pub_date = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
  platforms = [ordered]@{
    "windows-x86_64" = [ordered]@{
      signature = $signature
      url = "https://github.com/$RepoFullName/releases/download/$urlVersion/$assetName"
    }
  }
}

$latestJsonText = $latestJson | ConvertTo-Json -Depth 6
$utf8NoBom = [System.Text.UTF8Encoding]::new($false)
[System.IO.File]::WriteAllText($OutputPath, $latestJsonText, $utf8NoBom)

Write-Host "latest.json created: $OutputPath"
Write-Host "Update installer: $publishedBundlePath"
Write-Host "Update signature: $publishedSignaturePath"
Write-Host "GitHub asset name: $assetName"
