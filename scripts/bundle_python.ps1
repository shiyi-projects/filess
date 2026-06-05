# ──────────────────────────────────────────────────────────
# bundle_python.ps1
# Downloads Python Embeddable (Windows) and prepares
# python-runtime/ for Tauri resource bundling.
#
# Usage:
#   .\scripts\bundle_python.ps1                  # default 3.11.9 amd64
#   .\scripts\bundle_python.ps1 -Version 3.12.7  # specific version
# ──────────────────────────────────────────────────────────

[CmdletBinding()]
param(
    [string]$Version = "3.11.9",
    [string]$Arch    = "amd64"
)

$ErrorActionPreference = "Stop"

$Root        = Split-Path -Parent $PSScriptRoot
$TauriDir    = Join-Path $Root   "apps\desktop\src-tauri"
$RuntimeDir  = Join-Path $TauriDir "python-runtime"
$SidecarSrc  = Join-Path $Root   "services\sidecar\src\sidecar"

# Short version for ._pth file lookup: "3.11.9" → "311"
$parts    = $Version -split '\.'
$ShortVer = "$($parts[0])$($parts[1])"

$ZipName = "python-$Version-embed-$Arch.zip"
$Url     = "https://www.python.org/ftp/python/$Version/$ZipName"
$TempZip = Join-Path $TauriDir $ZipName

Write-Host ""
Write-Host "=== Bundle Python Embeddable ===" -ForegroundColor Cyan
Write-Host "  Version : $Version ($Arch)"
Write-Host "  URL     : $Url"
Write-Host "  Target  : $RuntimeDir"
Write-Host ""

# ── 1. Clean previous build ─────────────────────────────
if (Test-Path $RuntimeDir) {
    Write-Host "-> Removing old python-runtime/ ..."
    Remove-Item -Recurse -Force $RuntimeDir
}

# ── 2. Download (with cache) ────────────────────────────
if (-not (Test-Path $TempZip)) {
    Write-Host "-> Downloading $ZipName ..."
    Invoke-WebRequest -Uri $Url -OutFile $TempZip -UseBasicParsing
    Write-Host "   Downloaded $([math]::Round((Get-Item $TempZip).Length / 1MB, 1)) MB"
} else {
    Write-Host "-> Using cached $ZipName"
}

# ── 3. Extract ──────────────────────────────────────────
Write-Host "-> Extracting to python-runtime/ ..."
Expand-Archive -Path $TempZip -DestinationPath $RuntimeDir -Force

# ── 4. Patch ._pth to keep "." in sys.path ─────────────
# The default python3XX._pth already contains "." which maps to the
# directory of python.exe (= python-runtime/). Since we place the
# sidecar package directly inside python-runtime/sidecar/, imports
# like `import sidecar.rpc.dispatcher` resolve naturally.
#
# We also add `import site` so PYTHONPATH (set by our Rust code)
# is honoured as a fallback — this covers edge cases without
# breaking the embedded isolation.
$PthFile = Join-Path $RuntimeDir "python$ShortVer._pth"
if (Test-Path $PthFile) {
    Write-Host "-> Patching $([System.IO.Path]::GetFileName($PthFile))"
    $content = Get-Content $PthFile -Raw -Encoding UTF8
    if ($content -notmatch 'import site') {
        Add-Content -Path $PthFile -Value "import site" -Encoding UTF8
    }
} else {
    Write-Host "!! Warning: $PthFile not found — PYTHONPATH may not work" -ForegroundColor Yellow
}

# ── 5. Copy sidecar source ──────────────────────────────
Write-Host "-> Copying sidecar source code ..."
$SidecarDest = Join-Path $RuntimeDir "sidecar"
Copy-Item -Recurse -Force $SidecarSrc $SidecarDest

# Remove __pycache__ directories (not needed at runtime)
Get-ChildItem -Path $SidecarDest -Recurse -Directory -Filter "__pycache__" |
    ForEach-Object { Remove-Item -Recurse -Force $_.FullName }

# ── 6. Summary ──────────────────────────────────────────
$totalSize = (Get-ChildItem -Recurse -File $RuntimeDir |
    Measure-Object -Property Length -Sum).Sum
$fileCount = (Get-ChildItem -Recurse -File $RuntimeDir).Count

Write-Host ""
Write-Host "OK  python-runtime/ ready" -ForegroundColor Green
Write-Host "    Files : $fileCount"
Write-Host "    Size  : $([math]::Round($totalSize / 1MB, 1)) MB"
Write-Host ""
