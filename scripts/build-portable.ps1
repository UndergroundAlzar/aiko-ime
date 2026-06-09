# AikoIME - Portable Build Script
# AikoIME 便携版打包脚本

param(
    [switch]$Clean = $false,
    [string]$Version = "1.0.0"
)

$ErrorActionPreference = "Stop"

Write-Host "🔧 Building AikoIME v$Version..." -ForegroundColor Cyan

# Clean if requested
if ($Clean) {
    Write-Host "🧹 Cleaning previous build..." -ForegroundColor Yellow
    cargo clean
}

# Build release version with static linking
Write-Host "🏗️ Building release version..." -ForegroundColor Yellow
$env:RUSTFLAGS = "-C target-feature=+crt-static"
$env:CMAKE_POLICY_VERSION_MINIMUM = "3.5"
cargo build --release --target x86_64-pc-windows-msvc

if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ Build failed!" -ForegroundColor Red
    exit 1
}

# Create portable directory
$PortableDir = "dist\aiko-ime-portable"
Write-Host "📁 Creating portable directory: $PortableDir" -ForegroundColor Yellow

if (Test-Path $PortableDir) {
    Remove-Item -Recurse -Force $PortableDir
}
New-Item -ItemType Directory -Force -Path $PortableDir | Out-Null

# Copy main executable
$ExePath = "target\x86_64-pc-windows-msvc\release\aiko-ime.exe"
if (Test-Path $ExePath) {
    Copy-Item $ExePath $PortableDir
    Write-Host "✅ Copied executable" -ForegroundColor Green
} else {
    Write-Host "❌ Executable not found: $ExePath" -ForegroundColor Red
    exit 1
}

# Copy configuration template
if (Test-Path "config.toml.example") {
    Copy-Item "config.toml.example" "$PortableDir\config.toml"
    Write-Host "✅ Copied configuration" -ForegroundColor Green
}

# Copy README
if (Test-Path "README.md") {
    Copy-Item "README.md" $PortableDir
    Write-Host "✅ Copied README" -ForegroundColor Green
}

# Copy release notes
if (Test-Path "RELEASE_NOTES.md") {
    Copy-Item "RELEASE_NOTES.md" $PortableDir
    Write-Host "✅ Copied release notes" -ForegroundColor Green
}

# Copy README image assets
if (Test-Path "assets\aiko_readme_showcase.png") {
    New-Item -ItemType Directory -Force -Path "$PortableDir\assets" | Out-Null
    Copy-Item "assets\aiko_readme_showcase.png" "$PortableDir\assets\"
    Write-Host "✅ Copied README image assets" -ForegroundColor Green
}

# Create version file
"v$Version" | Out-File "$PortableDir\VERSION.txt" -Encoding UTF8

# Create ZIP archive
$ZipPath = "aiko-ime-v$Version-portable.zip"
Write-Host "📦 Creating ZIP archive: $ZipPath" -ForegroundColor Yellow

if (Test-Path $ZipPath) {
    Remove-Item $ZipPath
}
Compress-Archive -Path $PortableDir -DestinationPath $ZipPath -Force

# Get file size
$ExeSize = (Get-Item "$PortableDir\aiko-ime.exe").Length / 1MB
$ZipSize = (Get-Item $ZipPath).Length / 1MB

Write-Host ""
Write-Host "✅ Build completed successfully!" -ForegroundColor Green
Write-Host ""
Write-Host "📊 Build Statistics:" -ForegroundColor Cyan
Write-Host "   Executable size: $([math]::Round($ExeSize, 2)) MB"
Write-Host "   Archive size:    $([math]::Round($ZipSize, 2)) MB"
Write-Host ""
Write-Host "📁 Output files:" -ForegroundColor Cyan
Write-Host "   $PortableDir\"
Write-Host "   $ZipPath"
Write-Host ""
