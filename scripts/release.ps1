# Release script: bump version -> commit -> tag -> push, triggering the
# GitHub Actions workflow that builds the macOS Release.
#
# Usage:
#   .\scripts\release.ps1 0.1.1                   # release a new version
#   .\scripts\release.ps1                         # reuse current version (re-tag only)
#   .\scripts\release.ps1 0.1.1 -Message "note"   # custom commit message

[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [string]$Version = "",

    [Alias("m")]
    [string]$Message = ""
)

$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

$TauriConf  = "apps/desktop/src-tauri/tauri.conf.json"
$RootPkg    = "package.json"
$DesktopPkg = "apps/desktop/package.json"

function Read-JsonVersion($path) {
    $json = Get-Content -Raw -Encoding UTF8 -Path $path | ConvertFrom-Json
    return $json.version
}

function Write-JsonVersion($path, $newVersion) {
    # Use python to rewrite — avoids PowerShell ConvertTo-Json escape/indent quirks
    # (ConvertTo-Json on PS 5.1 escapes non-ASCII chars like the Chinese productName).
    $pyScript = @'
import json, sys
new, path = sys.argv[1], sys.argv[2]
with open(path, 'r', encoding='utf-8') as f:
    data = json.load(f)
data['version'] = new
with open(path, 'w', encoding='utf-8', newline='\n') as f:
    json.dump(data, f, ensure_ascii=False, indent=2)
    f.write('\n')
'@
    # Pipe the script to python's stdin (the `-` arg tells python to read stdin).
    $pyScript | & python - $newVersion $path
    if ($LASTEXITCODE -ne 0) { throw "Failed to write $path" }
}

function Confirm-Yes($prompt) {
    $ans = Read-Host "$prompt [y/N]"
    return $ans -match '^[Yy]$'
}

function Invoke-Git {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments,

        [switch]$IgnoreFailure,
        [switch]$Quiet
    )

    $stdout = [System.IO.Path]::GetTempFileName()
    $stderr = [System.IO.Path]::GetTempFileName()
    try {
        $proc = Start-Process -FilePath "git" `
            -ArgumentList $Arguments `
            -NoNewWindow `
            -Wait `
            -PassThru `
            -RedirectStandardOutput $stdout `
            -RedirectStandardError $stderr

        $outLines = if ((Get-Item $stdout).Length -gt 0) { Get-Content $stdout } else { @() }
        $errLines = if ((Get-Item $stderr).Length -gt 0) { Get-Content $stderr } else { @() }

        if (-not $Quiet) {
            foreach ($line in $outLines) { Write-Host $line }
            foreach ($line in $errLines) { Write-Host $line }
        }

        if ($proc.ExitCode -ne 0 -and -not $IgnoreFailure) {
            $joined = ($Arguments -join " ")
            throw "git $joined failed with exit code $($proc.ExitCode)"
        }

        return @($outLines + $errLines)
    } finally {
        Remove-Item $stdout, $stderr -ErrorAction SilentlyContinue
    }
}

# ---- Read current version ----
$current = Read-JsonVersion $TauriConf
$target  = if ([string]::IsNullOrWhiteSpace($Version)) { $current } else { $Version }
$tag     = "v$target"

Write-Host "-> Current version: $current"
Write-Host "-> Target version : $target  (tag: $tag)"

# ---- Validate version format ----
if ($target -notmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$') {
    Write-Error "Invalid version: $target  (expected X.Y.Z or X.Y.Z-beta.1)"
    exit 1
}

# ---- Require clean working tree ----
$status = git status --porcelain
if ($status) {
    Write-Host "x Working tree has uncommitted changes. Commit or stash first." -ForegroundColor Red
    git status --short
    exit 1
}

# ---- Branch check ----
$branch = (git rev-parse --abbrev-ref HEAD).Trim()
if ($branch -ne "main" -and $branch -ne "master") {
    if (-not (Confirm-Yes "! Current branch is '$branch' (not main/master). Continue?")) {
        Write-Host "Aborted."; exit 1
    }
}

# ---- Sync version into package.json files ----
if ($target -ne $current) {
    Write-Host "-> Updating $TauriConf / $RootPkg / $DesktopPkg"
    Write-JsonVersion $TauriConf  $target
    Write-JsonVersion $RootPkg    $target
    Write-JsonVersion $DesktopPkg $target

    git add $TauriConf $RootPkg $DesktopPkg
    $msg = if ([string]::IsNullOrWhiteSpace($Message)) { "chore: release $tag" } else { $Message }
    git commit -m $msg
    if ($LASTEXITCODE -ne 0) { throw "git commit failed" }
}

# ---- Handle existing tag ----
# Use `git tag -l` instead of `git rev-parse` — it returns empty + exit 0 when
# the tag doesn't exist, avoiding PS 5.1's NativeCommandError on stderr.
$existingTag = Invoke-Git -Arguments @("tag", "-l", $tag) -Quiet    # empty string / $null when missing
if ($existingTag) {
    if (Confirm-Yes "! Local tag $tag already exists. Delete and recreate?") {
        Invoke-Git -Arguments @("tag", "-d", $tag) -Quiet | Out-Null
        # Best-effort delete on remote; ignore failure (tag may not exist there).
        Invoke-Git -Arguments @("push", "origin", ":refs/tags/$tag") -IgnoreFailure -Quiet | Out-Null
    } else {
        Write-Host "Aborted."; exit 1
    }
}

# ---- Push branch and tag ----
Write-Host "-> Pushing commit to origin/$branch"
Invoke-Git -Arguments @("push", "origin", $branch) | Out-Null

Write-Host "-> Creating and pushing tag $tag"
git tag -a $tag -m "Release $tag"
Invoke-Git -Arguments @("push", "origin", $tag) | Out-Null

# ---- Output links ----
$remote = ((Invoke-Git -Arguments @("remote", "get-url", "origin") -Quiet) -join "").Trim()
$repoUrl = $remote `
    -replace '^git@github\.com:', 'https://github.com/' `
    -replace '^https://github\.com/', 'https://github.com/' `
    -replace '\.git$', ''

Write-Host ""
Write-Host "OK  Pushed $tag" -ForegroundColor Green
Write-Host "    Actions : $repoUrl/actions"
Write-Host "    Releases: $repoUrl/releases"
