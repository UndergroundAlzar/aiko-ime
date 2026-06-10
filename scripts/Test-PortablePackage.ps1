# Validates an unpacked Aiko IME portable directory or a portable ZIP archive.

[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$PackagePath,
    [string]$SourceRoot
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ([string]::IsNullOrWhiteSpace($SourceRoot)) {
    $SourceRoot = Split-Path -Parent $PSScriptRoot
}
$SourceRoot = [IO.Path]::GetFullPath($SourceRoot)
if ([IO.Path]::IsPathFullyQualified($PackagePath)) {
    $PackagePath = [IO.Path]::GetFullPath($PackagePath)
}
else {
    $PackagePath = [IO.Path]::GetFullPath((Join-Path (Get-Location) $PackagePath))
}
$ManifestPath = Join-Path $SourceRoot "tests\portable-layout.json"
$Manifest = Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json
$TemporaryDirectory = $null

function Assert-Condition {
    param(
        [Parameter(Mandatory)]
        [bool]$Condition,
        [Parameter(Mandatory)]
        [string]$Message
    )
    if (-not $Condition) {
        throw $Message
    }
}

try {
    Assert-Condition (Test-Path -LiteralPath $PackagePath) "Package path does not exist: $PackagePath"
    Assert-Condition ($Manifest.requiredFiles -contains "config.toml") "Portable manifest must require config.toml."

    if ([IO.Path]::GetExtension($PackagePath) -ieq ".zip") {
        $TemporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) ("aiko-ime-package-" + [guid]::NewGuid())
        New-Item -ItemType Directory -Path $TemporaryDirectory | Out-Null
        Expand-Archive -LiteralPath $PackagePath -DestinationPath $TemporaryDirectory
        $PackageRoot = $TemporaryDirectory

        $topLevel = @(Get-ChildItem -LiteralPath $PackageRoot -Force)
        if ($topLevel.Count -eq 1 -and $topLevel[0].PSIsContainer) {
            $candidate = $topLevel[0].FullName
            if (Test-Path -LiteralPath (Join-Path $candidate "aiko-ime.exe")) {
                $PackageRoot = $candidate
            }
        }
    }
    else {
        Assert-Condition (Test-Path -LiteralPath $PackagePath -PathType Container) `
            "PackagePath must be a directory or ZIP archive: $PackagePath"
        $PackageRoot = $PackagePath
    }

    foreach ($relativePath in $Manifest.requiredFiles) {
        $fullPath = Join-Path $PackageRoot ([string]$relativePath)
        Assert-Condition (Test-Path -LiteralPath $fullPath -PathType Leaf) `
            "Portable package is missing required file: $relativePath"
        Assert-Condition ((Get-Item -LiteralPath $fullPath).Length -gt 0) `
            "Portable package contains an empty required file: $relativePath"
    }

    foreach ($relativePath in $Manifest.requiredDirectories) {
        $fullPath = Join-Path $PackageRoot ([string]$relativePath)
        Assert-Condition (Test-Path -LiteralPath $fullPath -PathType Container) `
            "Portable package is missing required directory: $relativePath"
        Assert-Condition (@(Get-ChildItem -LiteralPath $fullPath -File -Recurse).Count -gt 0) `
            "Portable package contains an empty required directory: $relativePath"
    }

    $exePath = Join-Path $PackageRoot "aiko-ime.exe"
    $header = [IO.File]::ReadAllBytes($exePath)
    Assert-Condition ($header.Length -gt 1024) "aiko-ime.exe is unexpectedly small."
    Assert-Condition ($header[0] -eq 0x4d -and $header[1] -eq 0x5a) `
        "aiko-ime.exe does not have a valid PE/DOS MZ header."

    $cargoVersionMatch = Select-String -Path (Join-Path $SourceRoot "Cargo.toml") `
        -Pattern '^version\s*=\s*"([^"]+)"$' | Select-Object -First 1
    Assert-Condition ($null -ne $cargoVersionMatch) "Unable to read Cargo.toml package version."
    $cargoVersion = $cargoVersionMatch.Matches[0].Groups[1].Value
    $packagedVersion = (Get-Content -LiteralPath (Join-Path $PackageRoot "VERSION.txt") -Raw).Trim()
    Assert-Condition ($packagedVersion -eq "v$cargoVersion") `
        "VERSION.txt '$packagedVersion' does not match Cargo.toml version 'v$cargoVersion'."

    $sourceAssets = Join-Path $SourceRoot "assets"
    $packageAssets = Join-Path $PackageRoot "assets"

    foreach ($sourceAsset in Get-ChildItem -LiteralPath $sourceAssets -File -Recurse) {
        $relative = [IO.Path]::GetRelativePath($sourceAssets, $sourceAsset.FullName)
        $packagedAsset = Join-Path $packageAssets $relative
        Assert-Condition (Test-Path -LiteralPath $packagedAsset -PathType Leaf) `
            "Portable package is missing nested asset: assets/$($relative.Replace('\', '/'))"
        $sourceHash = (Get-FileHash -LiteralPath $sourceAsset.FullName -Algorithm SHA256).Hash
        $packageHash = (Get-FileHash -LiteralPath $packagedAsset -Algorithm SHA256).Hash
        Assert-Condition ($sourceHash -eq $packageHash) `
            "Packaged asset differs from source: assets/$($relative.Replace('\', '/'))"
    }

    $readme = Get-Content -LiteralPath (Join-Path $PackageRoot "README.md") -Raw
    $imageReferences = [regex]::Matches($readme, '!\[[^\]]*\]\((assets/[^)\s]+)\)')
    foreach ($reference in $imageReferences) {
        $relative = $reference.Groups[1].Value.Replace('/', [IO.Path]::DirectorySeparatorChar)
        Assert-Condition (Test-Path -LiteralPath (Join-Path $PackageRoot $relative) -PathType Leaf) `
            "README references an asset that is absent from the package: $relative"
    }

    Write-Host "Portable package validation passed: $PackagePath"
}
finally {
    if ($TemporaryDirectory -and (Test-Path -LiteralPath $TemporaryDirectory)) {
        $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/')
        $resolvedTemp = [IO.Path]::GetFullPath($TemporaryDirectory)
        if ($resolvedTemp.StartsWith("$tempRoot\", [StringComparison]::OrdinalIgnoreCase)) {
            Remove-Item -LiteralPath $resolvedTemp -Recurse -Force
        }
    }
}
