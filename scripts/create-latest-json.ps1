param(
  [string]$RepoFullName = "RickChen764/tokenscope-desktop-public",
  [string]$Version,
  [string]$Notes = "TokenScope Desktop updater package.",
  [string]$NotesPath,
  [string]$BundleDir,
  [string]$OutputPath
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

function Read-Utf8Text {
  param([string]$Path)

  return [System.IO.File]::ReadAllText([System.IO.Path]::GetFullPath($Path), $script:Utf8NoBom)
}

function Write-Utf8Text {
  param(
    [string]$Path,
    [string]$Text
  )

  [System.IO.File]::WriteAllText([System.IO.Path]::GetFullPath($Path), $Text, $script:Utf8NoBom)
}

function Read-JsonFile {
  param([string]$Path)

  return Read-Utf8Text -Path $Path | ConvertFrom-Json
}

function Get-ProjectRoot {
  return [System.IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
}

function Get-TauriVersion {
  param([string]$Root)

  $configPath = Join-Path $Root "src-tauri\tauri.conf.json"
  $config = Read-JsonFile -Path $configPath
  return [string]$config.version
}

function Get-PackageVersion {
  param([string]$Root)

  $packagePath = Join-Path $Root "package.json"
  $package = Read-JsonFile -Path $packagePath
  return [string]$package.version
}

function Assert-VersionConsistency {
  param(
    [string]$Root,
    [string]$ExpectedVersion
  )

  $tauriVersion = Get-TauriVersion -Root $Root
  $packageVersion = Get-PackageVersion -Root $Root

  if ($tauriVersion -ne $packageVersion) {
    throw "Version mismatch: package.json is $packageVersion but tauri.conf.json is $tauriVersion"
  }

  if ($ExpectedVersion -ne $tauriVersion) {
    throw "Version mismatch: requested version $ExpectedVersion but tauri.conf.json is $tauriVersion"
  }
}

function Assert-ReleaseArtifact {
  param(
    [string]$Path,
    [string]$Name
  )

  if (-not (Test-Path -LiteralPath $Path)) {
    throw "Missing $Name`: $Path"
  }

  $item = Get-Item -LiteralPath $Path
  if ($item.Length -le 0) {
    throw "$Name is empty: $Path"
  }
}

function ConvertTo-GithubAssetName {
  param([string]$FileName)

  return $FileName -replace "\s+", "."
}

$root = Get-ProjectRoot
if (-not $Version) {
  $Version = Get-TauriVersion -Root $root
}
Assert-VersionConsistency -Root $root -ExpectedVersion $Version

if ($NotesPath) {
  $notesFullPath = [System.IO.Path]::GetFullPath($NotesPath)
  if (-not (Test-Path -LiteralPath $notesFullPath)) {
    throw "Missing release notes file: $notesFullPath"
  }

  $Notes = Read-Utf8Text -Path $notesFullPath
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
Assert-ReleaseArtifact -Path $updateBundle.FullName -Name "NSIS installer"

$signaturePath = "$($updateBundle.FullName).sig"
Assert-ReleaseArtifact -Path $signaturePath -Name "updater signature"

$assetName = ConvertTo-GithubAssetName -FileName $updateBundle.Name
$publishedBundlePath = Join-Path $BundleDir $assetName
$publishedSignaturePath = "$publishedBundlePath.sig"

if ($updateBundle.FullName -ne $publishedBundlePath) {
  Copy-Item -LiteralPath $updateBundle.FullName -Destination $publishedBundlePath -Force
  Copy-Item -LiteralPath $signaturePath -Destination $publishedSignaturePath -Force
}
Assert-ReleaseArtifact -Path $publishedBundlePath -Name "published installer"
Assert-ReleaseArtifact -Path $publishedSignaturePath -Name "published updater signature"

$urlVersion = if ($Version.StartsWith("v")) { $Version } else { "v$Version" }
$signature = (Read-Utf8Text -Path $signaturePath).Trim()
$installerSha256 = (Get-FileHash -LiteralPath $publishedBundlePath -Algorithm SHA256).Hash.ToLowerInvariant()

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
Write-Utf8Text -Path $OutputPath -Text $latestJsonText
$validatedLatestJsonText = Read-Utf8Text -Path $OutputPath
$validatedLatestJson = $validatedLatestJsonText | ConvertFrom-Json
if ($validatedLatestJson.version -ne $Version) {
  throw "latest.json validation failed: expected version $Version"
}
Assert-ReleaseArtifact -Path $OutputPath -Name "latest.json"

Write-Host "latest.json created: $OutputPath"
Write-Host "Update installer: $publishedBundlePath"
Write-Host "Update signature: $publishedSignaturePath"
Write-Host "Installer SHA256: $installerSha256"
Write-Host "GitHub asset name: $assetName"
