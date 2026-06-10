# Offline Windows smoke tests for the release binary and repository contracts.

[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$BinaryPath,
    [string]$PackagePath,
    [string]$SourceRoot
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ([string]::IsNullOrWhiteSpace($SourceRoot)) {
    $SourceRoot = Split-Path -Parent $PSScriptRoot
}
$SourceRoot = [IO.Path]::GetFullPath($SourceRoot)
if ([IO.Path]::IsPathFullyQualified($BinaryPath)) {
    $BinaryPath = [IO.Path]::GetFullPath($BinaryPath)
}
else {
    $BinaryPath = [IO.Path]::GetFullPath((Join-Path (Get-Location) $BinaryPath))
}
$Manifest = Get-Content -LiteralPath (Join-Path $SourceRoot "tests\portable-layout.json") -Raw |
    ConvertFrom-Json

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

function Test-ByteSequence {
    param(
        [Parameter(Mandatory)]
        [byte[]]$Bytes,
        [Parameter(Mandatory)]
        [byte[]]$Pattern
    )

    if ($Pattern.Length -eq 0 -or $Bytes.Length -lt $Pattern.Length) {
        return $false
    }

    $lastStart = $Bytes.Length - $Pattern.Length
    for ($offset = 0; $offset -le $lastStart; $offset++) {
        if ($Bytes[$offset] -ne $Pattern[0]) {
            continue
        }
        $matched = $true
        for ($index = 1; $index -lt $Pattern.Length; $index++) {
            if ($Bytes[$offset + $index] -ne $Pattern[$index]) {
                $matched = $false
                break
            }
        }
        if ($matched) {
            return $true
        }
    }
    return $false
}

function Test-BinaryString {
    param(
        [Parameter(Mandatory)]
        [byte[]]$Bytes,
        [Parameter(Mandatory)]
        [string]$Value
    )

    $ascii = [Text.Encoding]::ASCII.GetBytes($Value)
    $unicode = [Text.Encoding]::Unicode.GetBytes($Value)
    return (Test-ByteSequence -Bytes $Bytes -Pattern $ascii) -or
        (Test-ByteSequence -Bytes $Bytes -Pattern $unicode)
}

Assert-Condition ([Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
        [Runtime.InteropServices.OSPlatform]::Windows
    )) "Windows smoke tests must run on Windows."
Assert-Condition (Test-Path -LiteralPath $BinaryPath -PathType Leaf) `
    "Release binary not found: $BinaryPath"

$binary = [IO.File]::ReadAllBytes($BinaryPath)
Assert-Condition ($binary.Length -gt 1024) "Release binary is unexpectedly small."
Assert-Condition ($binary[0] -eq 0x4d -and $binary[1] -eq 0x5a) `
    "Release binary does not have a valid PE/DOS MZ header."

foreach ($windowClass in $Manifest.requiredWindowClasses) {
    Assert-Condition (Test-BinaryString -Bytes $binary -Value ([string]$windowClass)) `
        "Release binary is missing Win32 window class '$windowClass'."
}

$hotkeySource = Get-Content -LiteralPath (Join-Path $SourceRoot "src\business\hotkey_manager.rs") -Raw
foreach ($pattern in $Manifest.hotkeyStateMarkers) {
    Assert-Condition ($hotkeySource.Contains([string]$pattern)) `
        "Hotkey state machine is missing required marker '$pattern'."
}

$rustSources = (Get-ChildItem -LiteralPath (Join-Path $SourceRoot "src") -Filter "*.rs" -File -Recurse |
    ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw }) -join "`n"
$hasMutexApi = $rustSources.Contains("CreateMutexW")
$hasAlreadyExistsCheck = $rustSources.Contains("ERROR_ALREADY_EXISTS") -or
    ($rustSources -match 'raw_os_error\(\)\s*==\s*Some\(183\)')
Assert-Condition ($hasMutexApi -and $hasAlreadyExistsCheck) `
    "Single-instance guard is missing CreateMutexW plus ERROR_ALREADY_EXISTS (183) handling."
Assert-Condition (Test-BinaryString -Bytes $binary -Value ([string]$Manifest.singleInstanceMarker)) `
    "Release binary is missing single-instance marker '$($Manifest.singleInstanceMarker)'."

if (-not [string]::IsNullOrWhiteSpace($PackagePath)) {
    & "$PSScriptRoot\Test-PortablePackage.ps1" -PackagePath $PackagePath -SourceRoot $SourceRoot
}

Write-Host "Offline Windows smoke tests passed."
Write-Host "Checked: PE header, single-instance contract, hotkey state markers, window classes, and resources."
