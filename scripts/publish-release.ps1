param(
  [string]$Version = "auto",
  [string]$CommitMessage = "",
  [string]$ReleaseNotesPath = "",
  [string]$PrivateRemote = "private-origin",
  [string]$PrivateBranch = "private",
  [string]$PublicRemote = "origin",
  [string]$PublicBranch = "master",
  [string]$PublicWorktree = ".worktrees\public-master-sync",
  [string]$PublicRepo = "RickChen764/tokenscope-desktop-public",
  [string[]]$PublicExclude = @(".docs"),
  [switch]$SkipTests,
  [switch]$SkipBuild,
  [switch]$SkipRelease,
  [switch]$DryRun,
  [switch]$AllowDirty
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

$Root = Split-Path -Parent $PSScriptRoot
$PublicWorktreePath = if ([System.IO.Path]::IsPathRooted($PublicWorktree)) {
  [System.IO.Path]::GetFullPath($PublicWorktree)
} else {
  [System.IO.Path]::GetFullPath((Join-Path $Root $PublicWorktree))
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

function Invoke-Native {
  param(
    [string]$File,
    [string[]]$Arguments,
    [string]$WorkingDirectory = $Root
  )

  Push-Location $WorkingDirectory
  try {
    & $File @Arguments
    if ($LASTEXITCODE -ne 0) {
      throw "$File $($Arguments -join ' ') failed with exit code $LASTEXITCODE"
    }
  } finally {
    Pop-Location
  }
}

function Get-NativeLines {
  param(
    [string]$File,
    [string[]]$Arguments,
    [string]$WorkingDirectory = $Root
  )

  Push-Location $WorkingDirectory
  try {
    $output = & $File @Arguments
    if ($LASTEXITCODE -ne 0) {
      throw "$File $($Arguments -join ' ') failed with exit code $LASTEXITCODE"
    }
    return @($output | Where-Object { $_ -ne $null -and $_ -ne "" })
  } finally {
    Pop-Location
  }
}

function Get-GitLines {
  param(
    [string[]]$Arguments,
    [string]$WorkingDirectory = $Root
  )
  return Get-NativeLines -File "git" -Arguments $Arguments -WorkingDirectory $WorkingDirectory
}

function Assert-Tool {
  param([string]$Name)
  $tool = Get-Command $Name -ErrorAction SilentlyContinue
  if (-not $tool) {
    throw "Missing required tool: $Name"
  }
}

function Assert-CleanGitWorktree {
  param(
    [string]$Name,
    [string]$WorkingDirectory
  )

  $dirty = @(Get-GitLines -Arguments @("status", "--porcelain") -WorkingDirectory $WorkingDirectory)
  if ($dirty.Count -gt 0 -and -not $AllowDirty -and -not $DryRun) {
    $dirtyText = $dirty -join [Environment]::NewLine
    throw "$Name worktree is not clean. Commit or stash changes first:`n$dirtyText"
  }
  return $dirty
}

function Get-CurrentVersion {
  $package = Read-Utf8Text -Path (Join-Path $Root "package.json") | ConvertFrom-Json
  return [string]$package.version
}

function Get-NextPatchVersion {
  param([string]$CurrentVersion)

  $parts = $CurrentVersion.Split(".")
  if ($parts.Count -ne 3) {
    throw "Version must be semver x.y.z: $CurrentVersion"
  }
  return "$($parts[0]).$($parts[1]).$([int]$parts[2] + 1)"
}

function Replace-TextOrThrow {
  param(
    [string]$Path,
    [string]$Pattern,
    [string]$Replacement
  )

  $text = Read-Utf8Text -Path $Path
  $updated = [regex]::Replace($text, $Pattern, $Replacement)
  if ($updated -eq $text) {
    throw "No replacement made in $Path"
  }
  Write-Utf8Text -Path $Path -Text $updated
}

function Set-AppVersion {
  param([string]$TargetVersion)

  $jsonVersionPattern = '("version"\s*:\s*")\d+\.\d+\.\d+(")'
  $cargoTomlVersionPattern = '(?m)^(version = ")\d+\.\d+\.\d+(")'
  $cargoLockVersionPattern = '(?s)(name = "tokenscope-desktop"\s+version = ")\d+\.\d+\.\d+(")'
  $replacement = '${1}' + $TargetVersion + '${2}'

  Replace-TextOrThrow -Path (Join-Path $Root "package.json") -Pattern $jsonVersionPattern -Replacement $replacement
  Replace-TextOrThrow -Path (Join-Path $Root "src-tauri\tauri.conf.json") -Pattern $jsonVersionPattern -Replacement $replacement
  Replace-TextOrThrow -Path (Join-Path $Root "src-tauri\Cargo.toml") -Pattern $cargoTomlVersionPattern -Replacement $replacement
  Replace-TextOrThrow -Path (Join-Path $Root "src-tauri\Cargo.lock") -Pattern $cargoLockVersionPattern -Replacement $replacement
}

function Get-ReleaseBullets {
  param([string[]]$Subjects)

  if ($Subjects.Count -gt 0) {
    return @($Subjects | ForEach-Object { "- $_" })
  }
  return @("- 更新发布版本信息。")
}

function Update-Changelog {
  param(
    [string]$TargetVersion,
    [string[]]$Bullets
  )

  $path = Join-Path $Root "CHANGELOG.md"
  $text = Read-Utf8Text -Path $path
  $date = Get-Date -Format "yyyy-MM-dd"
  $entryLines = @("### $TargetVersion 更新包", "") + $Bullets + @(
    "- 版本提升到 $TargetVersion，用于发布新的 Windows NSIS 安装包、Tauri updater 签名文件和 latest.json。",
    ""
  )
  $entry = ($entryLines -join "`n")
  $dateHeader = "## $date"
  $dateIndex = $text.IndexOf($dateHeader)

  if ($dateIndex -ge 0) {
    $insertAt = $text.IndexOf("`n", $dateIndex)
    if ($insertAt -lt 0) {
      $updated = "$text`n`n$entry"
    } else {
      $updated = $text.Insert($insertAt + 1, "`n$entry")
    }
  } else {
    $title = "# 变更日志`n`n"
    if (-not $text.StartsWith($title)) {
      throw "CHANGELOG.md does not start with expected title"
    }
    $updated = $text.Replace($title, "$title## $date`n`n$entry")
  }

  Write-Utf8Text -Path $path -Text $updated
}

function New-ReleaseNotesFile {
  param(
    [string]$TargetVersion,
    [string[]]$Bullets
  )

  if ($ReleaseNotesPath) {
    $resolved = [System.IO.Path]::GetFullPath((Join-Path $Root $ReleaseNotesPath))
    if (-not (Test-Path -LiteralPath $resolved)) {
      throw "Release notes file does not exist: $resolved"
    }
    return $resolved
  }

  $path = Join-Path $env:TEMP "tokenscope-v$TargetVersion-notes.md"
  $text = (@("TokenScope Desktop $TargetVersion", "") + $Bullets) -join "`n"
  Write-Utf8Text -Path $path -Text "$text`n"
  return $path
}

function Invoke-ProjectTests {
  param([string]$WorkingDirectory)

  Invoke-Native -File "pnpm" -Arguments @("lint") -WorkingDirectory $WorkingDirectory
  Invoke-Native -File "pnpm" -Arguments @("test") -WorkingDirectory $WorkingDirectory
  Invoke-Native -File "cargo" -Arguments @("test") -WorkingDirectory (Join-Path $WorkingDirectory "src-tauri")
}

function Rewrite-StagedIndexToWorkingTree {
  param([string]$WorkingDirectory)

  $script = @'
const { execFileSync } = require("node:child_process");
const fs = require("node:fs");
const files = execFileSync("git", ["diff", "--cached", "--name-only"], { encoding: "utf8" })
  .split(/\r?\n/)
  .filter(Boolean);
for (const file of files) {
  const data = execFileSync("git", ["show", `:${file}`], {
    encoding: "buffer",
    maxBuffer: 128 * 1024 * 1024,
  });
  fs.writeFileSync(file, data);
}
'@
  Push-Location $WorkingDirectory
  try {
    $script | node -
    if ($LASTEXITCODE -ne 0) {
      throw "node staged-index rewrite failed"
    }
  } finally {
    Pop-Location
  }
}

function Remove-TrailingBlankLines {
  param([string]$Path)

  $text = Read-Utf8Text -Path $Path
  $updated = [regex]::Replace($text, '(\r?\n){2,}$', "`n")
  if ($updated -ne $text) {
    Write-Utf8Text -Path $Path -Text $updated
  }
}

function Assert-StagedDiffClean {
  param([string]$WorkingDirectory)

  $check = @(Get-NativeLines -File "git" -Arguments @("diff", "--check", "--cached") -WorkingDirectory $WorkingDirectory)
  if ($check.Count -eq 0) {
    return
  }
}

function Repair-And-Assert-StagedDiff {
  param([string]$WorkingDirectory)

  Push-Location $WorkingDirectory
  try {
    $output = & git diff --check --cached 2>&1
    if ($LASTEXITCODE -eq 0) {
      return
    }
    $text = ($output | Out-String)

    $blankLinePaths = @()
    foreach ($line in @($output)) {
      $lineText = [string]$line
      $match = [regex]::Match($lineText, '^(.*):\d+: new blank line at EOF\.?$')
      if ($match.Success) {
        $blankLinePaths += $match.Groups[1].Value
      }
    }
    $blankLinePaths = @($blankLinePaths | Select-Object -Unique)

    if ($blankLinePaths.Count -gt 0) {
      foreach ($relativePath in $blankLinePaths) {
        $diskPath = Join-Path $WorkingDirectory ($relativePath -replace '/', '\')
        Remove-TrailingBlankLines -Path $diskPath
      }
      Invoke-Native -File "git" -Arguments (@("add", "--") + $blankLinePaths) -WorkingDirectory $WorkingDirectory
      $output = & git diff --check --cached 2>&1
      if ($LASTEXITCODE -eq 0) {
        return
      }
      $text = ($output | Out-String)
    }

    throw "git diff --check --cached failed:`n$text"
  } finally {
    Pop-Location
  }
}

function Apply-PublicPatch {
  param([string]$TargetVersion)

  $patch = Join-Path $env:TEMP "tokenscope-public-$TargetVersion.patch"
  if (Test-Path -LiteralPath $patch) {
    Remove-Item -LiteralPath $patch -Force
  }

  $diffArgs = @("diff", "--binary", "--output=$patch", "$PublicRemote/$PublicBranch..$PrivateBranch", "--", ".")
  foreach ($exclude in $PublicExclude) {
    $diffArgs += ":!$exclude"
  }
  Invoke-Native -File "git" -Arguments $diffArgs -WorkingDirectory $PublicWorktreePath
  Invoke-Native -File "git" -Arguments @("apply", "--check", "--whitespace=nowarn", $patch) -WorkingDirectory $PublicWorktreePath
  Invoke-Native -File "git" -Arguments @("apply", "--index", "--whitespace=nowarn", $patch) -WorkingDirectory $PublicWorktreePath

  $privateMatches = @(Get-GitLines -Arguments @("diff", "--cached", "--name-only") -WorkingDirectory $PublicWorktreePath |
    Where-Object { $_ -like ".docs/*" -or $_ -eq ".docs" })
  if ($privateMatches.Count -gt 0) {
    $privateMatchText = $privateMatches -join [Environment]::NewLine
    throw "Public patch contains private docs:`n$privateMatchText"
  }

  Rewrite-StagedIndexToWorkingTree -WorkingDirectory $PublicWorktreePath
  $stagedFiles = @(Get-GitLines -Arguments @("diff", "--cached", "--name-only") -WorkingDirectory $PublicWorktreePath)
  if ($stagedFiles.Count -eq 0) {
    throw "Public patch produced no staged changes"
  }
  Invoke-Native -File "git" -Arguments (@("add", "--") + $stagedFiles) -WorkingDirectory $PublicWorktreePath
  Repair-And-Assert-StagedDiff -WorkingDirectory $PublicWorktreePath
}

function Get-Sha256 {
  param([string]$Path)

  $stream = [System.IO.File]::OpenRead([System.IO.Path]::GetFullPath($Path))
  try {
    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
      $hash = $sha256.ComputeHash($stream)
      return (($hash | ForEach-Object { $_.ToString("x2") }) -join "")
    } finally {
      $sha256.Dispose()
    }
  } finally {
    $stream.Dispose()
  }
}

Assert-Tool -Name "git"
Assert-Tool -Name "gh"
Assert-Tool -Name "pnpm"
Assert-Tool -Name "cargo"
Assert-Tool -Name "node"

Invoke-Native -File "git" -Arguments @("fetch", $PrivateRemote, "+refs/heads/*:refs/remotes/$PrivateRemote/*", "--prune", "--no-tags")
Invoke-Native -File "git" -Arguments @("fetch", $PublicRemote, "+refs/heads/*:refs/remotes/$PublicRemote/*", "--prune", "--no-tags")

$privateDirty = Assert-CleanGitWorktree -Name "Private" -WorkingDirectory $Root
$publicDirty = Assert-CleanGitWorktree -Name "Public" -WorkingDirectory $PublicWorktreePath
$currentVersion = Get-CurrentVersion
$targetVersion = if ($Version -eq "auto" -or $Version -eq "") {
  Get-NextPatchVersion -CurrentVersion $currentVersion
} else {
  $Version
}

if ($targetVersion -notmatch '^\d+\.\d+\.\d+$') {
  throw "Target version must be semver x.y.z: $targetVersion"
}

$tag = "v$targetVersion"
$existingTag = @(Get-NativeLines -File "git" -Arguments @("ls-remote", "--tags", $PublicRemote, $tag))
if ($existingTag.Count -gt 0) {
  throw "Public tag already exists: $tag"
}

$subjects = @(Get-GitLines -Arguments @("log", "--reverse", "--format=%s", "$PrivateRemote/$PrivateBranch..$PrivateBranch"))
$bullets = Get-ReleaseBullets -Subjects $subjects
$summarySubject = if ($CommitMessage) {
  $CommitMessage
} elseif ($subjects.Count -gt 0) {
  [string]$subjects[-1]
} else {
  "更新 $targetVersion 发布版本信息"
}
$releaseCommitMessage = "记录 $summarySubject 版本信息"
$notesPath = New-ReleaseNotesFile -TargetVersion $targetVersion -Bullets $bullets

if ($DryRun) {
  [ordered]@{
    dryRun = $true
    currentVersion = $currentVersion
    targetVersion = $targetVersion
    privateDirty = $privateDirty
    publicDirty = $publicDirty
    unpushedSubjects = $subjects
    releaseNotesPath = $notesPath
    privateReleaseCommitMessage = $releaseCommitMessage
    publicCommitMessage = $summarySubject
  } | ConvertTo-Json -Depth 5
  exit 0
}

Set-AppVersion -TargetVersion $targetVersion
Update-Changelog -TargetVersion $targetVersion -Bullets $bullets

if (-not $SkipTests) {
  Invoke-ProjectTests -WorkingDirectory $Root
}

if (-not $SkipBuild) {
  Invoke-Native -File "powershell" -Arguments @(
    "-NoProfile",
    "-ExecutionPolicy",
    "Bypass",
    "-File",
    (Join-Path $PSScriptRoot "build-release.ps1"),
    "-NotesPath",
    $notesPath
  )
}

Invoke-Native -File "git" -Arguments @(
  "add",
  "--",
  "CHANGELOG.md",
  "package.json",
  "src-tauri/Cargo.lock",
  "src-tauri/Cargo.toml",
  "src-tauri/tauri.conf.json"
)
Invoke-Native -File "git" -Arguments @("commit", "-m", $releaseCommitMessage)
Invoke-Native -File "git" -Arguments @("push", $PrivateRemote, $PrivateBranch)

Invoke-Native -File "git" -Arguments @("fetch", $PublicRemote, "+refs/heads/*:refs/remotes/$PublicRemote/*", "--prune", "--no-tags") -WorkingDirectory $PublicWorktreePath
Apply-PublicPatch -TargetVersion $targetVersion
if (-not $SkipTests) {
  Invoke-ProjectTests -WorkingDirectory $PublicWorktreePath
}
Invoke-Native -File "git" -Arguments @("commit", "-m", $summarySubject) -WorkingDirectory $PublicWorktreePath
Invoke-Native -File "git" -Arguments @("push", $PublicRemote, $PublicBranch) -WorkingDirectory $PublicWorktreePath

$bundleDir = Join-Path $Root "src-tauri\target\release\bundle\nsis"
$installer = Join-Path $bundleDir "TokenScope.Desktop_$($targetVersion)_x64-setup.exe"
$signature = Join-Path $bundleDir "TokenScope.Desktop_$($targetVersion)_x64-setup.exe.sig"
$latest = Join-Path $bundleDir "latest.json"
$releaseUrl = ""
if (-not $SkipRelease) {
  foreach ($asset in @($installer, $signature, $latest)) {
    if (-not (Test-Path -LiteralPath $asset)) {
      throw "Missing release asset: $asset"
    }
  }
  $releaseArgs = @(
    "release",
    "create",
    $tag,
    "--repo",
    $PublicRepo,
    "--target",
    $PublicBranch,
    "--title",
    "TokenScope Desktop $targetVersion",
    "--notes-file",
    $notesPath,
    "--latest",
    $installer,
    $signature,
    $latest
  )
  $releaseUrl = (& gh @releaseArgs | Select-Object -Last 1)
  if ($LASTEXITCODE -ne 0) {
    throw "gh release create failed"
  }
}

$revParseHeadArgs = @("rev-parse", "--short", "HEAD")
$privateCommitLines = Get-GitLines -Arguments $revParseHeadArgs
$publicCommitLines = Get-GitLines -Arguments $revParseHeadArgs -WorkingDirectory $PublicWorktreePath
$privateCommit = $privateCommitLines[0]
$publicCommit = $publicCommitLines[0]
$installerSha256 = ""
$signatureSha256 = ""
$latestJsonSha256 = ""
if (Test-Path -LiteralPath $installer) {
  $installerSha256 = Get-Sha256 -Path $installer
}
if (Test-Path -LiteralPath $signature) {
  $signatureSha256 = Get-Sha256 -Path $signature
}
if (Test-Path -LiteralPath $latest) {
  $latestJsonSha256 = Get-Sha256 -Path $latest
}
$summary = [ordered]@{
  version = $targetVersion
  tag = $tag
  privateCommit = $privateCommit
  publicCommit = $publicCommit
  releaseUrl = $releaseUrl
  releaseNotesPath = $notesPath
  installer = $installer
  installerSha256 = $installerSha256
  signatureSha256 = $signatureSha256
  latestJsonSha256 = $latestJsonSha256
  publicExclude = $PublicExclude
}

$summary | ConvertTo-Json -Depth 5
