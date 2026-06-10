# Aiko IME portable build and packaging script.

[CmdletBinding()]
param(
    [switch]$Clean,
    [switch]$SkipBuild,
    [string]$Version,
    [string]$Target = "x86_64-pc-windows-msvc",
    [string]$OutputRoot = "dist"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$RepositoryRoot = Split-Path -Parent $PSScriptRoot
Set-Location $RepositoryRoot

function Get-CargoVersion {
    $match = Select-String -Path "Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"$' |
        Select-Object -First 1
    if (-not $match) {
        throw "Unable to read the package version from Cargo.toml."
    }
    return $match.Matches[0].Groups[1].Value
}

function Assert-ChildPath {
    param(
        [Parameter(Mandatory)]
        [string]$Path,
        [Parameter(Mandatory)]
        [string]$Parent
    )

    $parentFull = [IO.Path]::GetFullPath($Parent).TrimEnd('\', '/')
    $pathFull = [IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
    if (-not $pathFull.StartsWith("$parentFull\", [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to modify a path outside the output root: $pathFull"
    }
}

$CargoVersion = Get-CargoVersion
if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = $CargoVersion
}
$Version = $Version.TrimStart('v')
if ($Version -ne $CargoVersion) {
    throw "Requested version '$Version' does not match Cargo.toml version '$CargoVersion'."
}

if ([IO.Path]::IsPathFullyQualified($OutputRoot)) {
    $OutputRootFull = [IO.Path]::GetFullPath($OutputRoot)
}
else {
    $OutputRootFull = [IO.Path]::GetFullPath((Join-Path $RepositoryRoot $OutputRoot))
}
$PortableDir = Join-Path $OutputRootFull "aiko-ime-portable"
$ZipPath = Join-Path $OutputRootFull "aiko-ime-v$Version-portable.zip"
$ChecksumPath = "$ZipPath.sha256"
Assert-ChildPath -Path $PortableDir -Parent $OutputRootFull
Assert-ChildPath -Path $ZipPath -Parent $OutputRootFull

if ($Clean) {
    cargo clean
    if ($LASTEXITCODE -ne 0) {
        throw "cargo clean failed."
    }
}

if (-not $SkipBuild) {
    Write-Host "Building Aiko IME v$Version release..."
    $previousRustFlags = $env:RUSTFLAGS
    try {
        $env:RUSTFLAGS = "-C target-feature=+crt-static"
        $env:CMAKE_POLICY_VERSION_MINIMUM = "3.5"
        cargo build --locked --release --target $Target
        if ($LASTEXITCODE -ne 0) {
            throw "Release build failed."
        }
    }
    finally {
        $env:RUSTFLAGS = $previousRustFlags
    }
}

$ExePath = Join-Path $RepositoryRoot "target\$Target\release\aiko-ime.exe"
if (-not (Test-Path -LiteralPath $ExePath -PathType Leaf)) {
    throw "Release executable not found: $ExePath"
}

New-Item -ItemType Directory -Force -Path $OutputRootFull | Out-Null
if (Test-Path -LiteralPath $PortableDir) {
    Remove-Item -LiteralPath $PortableDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $PortableDir | Out-Null

Copy-Item -LiteralPath $ExePath -Destination (Join-Path $PortableDir "aiko-ime.exe")
Copy-Item -LiteralPath "config.toml.example" -Destination (Join-Path $PortableDir "config.toml")
Copy-Item -LiteralPath "README.md" -Destination $PortableDir
Copy-Item -LiteralPath "RELEASE_NOTES.md" -Destination $PortableDir
Copy-Item -LiteralPath "LICENSE" -Destination $PortableDir
Copy-Item -LiteralPath "assets" -Destination $PortableDir -Recurse
[IO.File]::WriteAllText((Join-Path $PortableDir "VERSION.txt"), "v$Version`r`n")

& "$PSScriptRoot\Test-PortablePackage.ps1" -PackagePath $PortableDir

if (Test-Path -LiteralPath $ZipPath) {
    Remove-Item -LiteralPath $ZipPath -Force
}
Compress-Archive -Path (Join-Path $PortableDir "*") -DestinationPath $ZipPath -CompressionLevel Optimal

& "$PSScriptRoot\Test-PortablePackage.ps1" -PackagePath $ZipPath

$hash = (Get-FileHash -LiteralPath $ZipPath -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText($ChecksumPath, "$hash  $([IO.Path]::GetFileName($ZipPath))`r`n")

$exeMb = [math]::Round((Get-Item -LiteralPath $ExePath).Length / 1MB, 2)
$zipMb = [math]::Round((Get-Item -LiteralPath $ZipPath).Length / 1MB, 2)
Write-Host "Portable package created and validated."
Write-Host "Executable: $ExePath ($exeMb MB)"
Write-Host "Archive:    $ZipPath ($zipMb MB)"
Write-Host "SHA-256:    $hash"
