# Builds the distributable Windows package of Anoxia Simulator.
# Usage: .\build-release.ps1 [-SkipTests]
param([switch]$SkipTests)

$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

# Version read from Cargo.toml (single source of truth)
$VERSION = (Select-String -Path "Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$RELEASE_NAME = "Anoxia-Simulator-v$VERSION-windows"
$DIST_DIR = "dist\$RELEASE_NAME"
$ZIP_PATH = "dist\$RELEASE_NAME.zip"

Write-Host "=== Anoxia Simulator $VERSION - build release ===" -ForegroundColor Cyan

if (-not $SkipTests) {
    Write-Host "[1/5] Tests and lint..." -ForegroundColor Yellow
    cargo test --workspace --release
    if ($LASTEXITCODE -ne 0) { throw "Tests failed." }
    cargo clippy --workspace --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw "Clippy reported warnings." }
}

Write-Host "[2/5] Release build..." -ForegroundColor Yellow
cargo build --release --bin anoxia-gui
if ($LASTEXITCODE -ne 0) { throw "Build failed." }
$exe = "target\release\anoxia-gui.exe"
if (-not (Test-Path $exe)) { throw "Executable not found: $exe" }

Write-Host "[3/5] Preparing $DIST_DIR..." -ForegroundColor Yellow
if (Test-Path $DIST_DIR) { Remove-Item -Recurse -Force $DIST_DIR }
if (Test-Path $ZIP_PATH) { Remove-Item -Force $ZIP_PATH }
New-Item -ItemType Directory -Path $DIST_DIR -Force | Out-Null
Copy-Item $exe "$DIST_DIR\Anoxia-Simulator.exe"
Copy-Item "README.md", "README.fr.md", "LICENSE", "CHANGELOG.md", "VALIDATION.md" "$DIST_DIR\"
# User guides and images
New-Item -ItemType Directory -Path "$DIST_DIR\docs" -Force | Out-Null
Copy-Item "docs\*.md" "$DIST_DIR\docs\"
Copy-Item "docs\img" "$DIST_DIR\docs\img" -Recurse

Write-Host "[4/5] ZIP archive..." -ForegroundColor Yellow
Compress-Archive -Path "$DIST_DIR\*" -DestinationPath $ZIP_PATH

Write-Host "[5/5] SHA-256 checksum..." -ForegroundColor Yellow
$hash = (Get-FileHash $ZIP_PATH -Algorithm SHA256).Hash.ToLower()
"$hash  $RELEASE_NAME.zip" | Out-File -Encoding ascii "$ZIP_PATH.sha256"

$size = [math]::Round((Get-Item $ZIP_PATH).Length / 1MB, 2)
Write-Host ""
Write-Host "Done: $ZIP_PATH ($size MB)" -ForegroundColor Green
Write-Host "SHA-256 : $hash"
Write-Host "Publishing: GitHub > Releases > tag v$VERSION, attach the ZIP and the .sha256."
