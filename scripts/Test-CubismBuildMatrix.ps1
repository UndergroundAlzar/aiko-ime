# Verifies that Cubism stays opt-in and that both no-SDK and no-model fallbacks remain buildable.

[CmdletBinding()]
param(
    [string]$SourceRoot,
    [switch]$SkipDefaultBuild
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ([string]::IsNullOrWhiteSpace($SourceRoot)) {
    $SourceRoot = Split-Path -Parent $PSScriptRoot
}
$SourceRoot = [IO.Path]::GetFullPath($SourceRoot)
Set-Location $SourceRoot

function Invoke-Cargo {
    param(
        [Parameter(Mandatory)]
        [string[]]$Arguments
    )

    Write-Host "cargo $($Arguments -join ' ')"
    & cargo @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "cargo $($Arguments -join ' ') failed with exit code $LASTEXITCODE."
    }
}

$metadata = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) {
    throw "cargo metadata failed."
}
$package = @($metadata.packages | Where-Object { $_.name -eq "aiko-ime" })
if ($package.Count -ne 1) {
    throw "Expected exactly one aiko-ime package in Cargo metadata."
}

$featureNames = @($package[0].features.PSObject.Properties.Name)
$cubismFeatures = @($featureNames | Where-Object {
        $_ -match "(?i)cubism|live2d"
    })
if ($cubismFeatures.Count -ne 1) {
    throw "Expected exactly one explicit Cubism/Live2D Cargo feature, found: $($cubismFeatures -join ', ')"
}
$cubismFeature = $cubismFeatures[0]
$defaultFeatures = @($package[0].features.default)
if ($defaultFeatures -contains $cubismFeature) {
    throw "Cubism feature '$cubismFeature' must remain opt-in."
}

$sdkVariables = @(
    "CUBISM_CORE_DLL",
    "CUBISM_SDK_ROOT",
    "CUBISM_SDK_DIR",
    "CUBISM_CORE_LIB_DIR",
    "CUBISM_SDK_NATIVE_DIR",
    "LIVE2D_CUBISM_SDK_ROOT",
    "LIVE2D_CUBISM_SDK_DIR",
    "LIVE2D_CUBISM_SDK_NATIVE_DIR"
)
$previousValues = @{}
foreach ($name in $sdkVariables) {
    $previousValues[$name] = [Environment]::GetEnvironmentVariable($name, "Process")
    [Environment]::SetEnvironmentVariable($name, $null, "Process")
}

try {
    if (-not $SkipDefaultBuild) {
        Invoke-Cargo -Arguments @(
            "check",
            "--locked",
            "--all-targets",
            "--no-default-features"
        )
    }

    # The feature build intentionally runs with every known SDK path unset. The
    # application must compile its PNG fallback instead of requiring proprietary
    # Cubism SDK files on developer and CI machines.
    Invoke-Cargo -Arguments @(
        "test",
        "--locked",
        "--all-targets",
        "--no-default-features",
        "--features",
        $cubismFeature
    )

    # This focused test exercises renderer selection with an absent model path.
    Invoke-Cargo -Arguments @(
        "test",
        "--locked",
        "--no-default-features",
        "cubism_renderer_falls_back_without_a_model"
    )
}
finally {
    foreach ($name in $sdkVariables) {
        [Environment]::SetEnvironmentVariable($name, $previousValues[$name], "Process")
    }
}

Write-Host "Cubism build matrix passed."
Write-Host "Checked feature: $cubismFeature"
Write-Host "Checked fallbacks: missing SDK and missing model."
