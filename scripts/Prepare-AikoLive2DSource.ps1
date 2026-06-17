# Prepares a reproducible Aiko Live2D modeler handoff workspace.

[CmdletBinding()]
param(
    [string]$RepositoryRoot,
    [Parameter(Mandatory)]
    [string]$WorkspaceRoot
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) {
    $RepositoryRoot = Split-Path -Parent $PSScriptRoot
}
$RepositoryRoot = [IO.Path]::GetFullPath($RepositoryRoot)
$WorkspaceRoot = [IO.Path]::GetFullPath($WorkspaceRoot)
$ManifestPath = Join-Path $RepositoryRoot "assets\live2d\aiko\source\production-manifest.json"

function Assert-Condition {
    param(
        [Parameter(Mandatory)][bool]$Condition,
        [Parameter(Mandatory)][string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Resolve-ContainedPath {
    param(
        [Parameter(Mandatory)][string]$Root,
        [Parameter(Mandatory)][string]$Relative,
        [Parameter(Mandatory)][string]$Context
    )

    Assert-Condition (-not [string]::IsNullOrWhiteSpace($Relative)) "$Context is empty."
    Assert-Condition (-not [IO.Path]::IsPathRooted($Relative)) "$Context must be relative: $Relative"
    Assert-Condition (-not $Relative.Contains("\")) "$Context must use forward slashes: $Relative"

    $rootFull = [IO.Path]::GetFullPath($Root).TrimEnd("\", "/")
    $candidate = [IO.Path]::GetFullPath((Join-Path $rootFull $Relative.Replace("/", "\")))
    Assert-Condition ($candidate.StartsWith(
            "$rootFull\",
            [StringComparison]::OrdinalIgnoreCase
        )) "$Context escapes its root: $Relative"
    return $candidate
}

function Read-JsonFile {
    param([Parameter(Mandatory)][string]$Path)

    Assert-Condition (Test-Path -LiteralPath $Path -PathType Leaf) "JSON file not found: $Path"
    try {
        return Get-Content -LiteralPath $Path -Raw -Encoding UTF8 | ConvertFrom-Json
    }
    catch {
        throw "Invalid JSON '$Path': $($_.Exception.Message)"
    }
}

function Get-Sha256 {
    param([Parameter(Mandatory)][string]$Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Copy-PinnedFile {
    param(
        [Parameter(Mandatory)][string]$SourceRelative,
        [Parameter(Mandatory)][string]$DestinationRelative,
        [string]$ExpectedSha256,
        [Parameter(Mandatory)][string]$Kind
    )

    $source = Resolve-ContainedPath $RepositoryRoot $SourceRelative "$Kind source"
    $destination = Resolve-ContainedPath $WorkspaceRoot $DestinationRelative "$Kind destination"
    Assert-Condition (Test-Path -LiteralPath $source -PathType Leaf) `
        "$Kind source file is missing: $SourceRelative"

    $sourceHash = Get-Sha256 $source
    if (-not [string]::IsNullOrWhiteSpace($ExpectedSha256)) {
        Assert-Condition ($sourceHash -eq $ExpectedSha256.ToLowerInvariant()) `
            "$Kind source hash differs from production-manifest.json: $SourceRelative"
    }

    $parent = Split-Path -Parent $destination
    [void](New-Item -ItemType Directory -Force -Path $parent)
    Copy-Item -LiteralPath $source -Destination $destination -Force
    Assert-Condition ((Get-Sha256 $destination) -eq $sourceHash) `
        "Copied $Kind differs from its source: $DestinationRelative"

    return [ordered]@{
        kind = $Kind
        source = $SourceRelative
        path = $DestinationRelative
        sha256 = $sourceHash
    }
}

Assert-Condition ($WorkspaceRoot -ne $RepositoryRoot) `
    "WorkspaceRoot must not be the repository root."

$production = Read-JsonFile $ManifestPath
Assert-Condition ([int]$production.schema_version -eq 1) `
    "production-manifest.json schema_version must be 1."
Assert-Condition ([string]$production.model_id -eq "aiko") `
    "production-manifest.json model_id must be 'aiko'."
foreach ($property in @(
        "creates_layered_psd",
        "creates_cmo3",
        "creates_moc3",
        "creates_model3",
        "performs_art_separation",
        "performs_rigging",
        "certifies_cubism_authenticity"
    )) {
    Assert-Condition (-not [bool]$production.automation_limits.$property) `
        "Automation limit '$property' must remain false."
}

[void](New-Item -ItemType Directory -Force -Path $WorkspaceRoot)
foreach ($relative in @($production.workspace_directories)) {
    $directory = Resolve-ContainedPath $WorkspaceRoot ([string]$relative) "workspace directory"
    [void](New-Item -ItemType Directory -Force -Path $directory)
}

$copiedFiles = @()
foreach ($reference in @($production.references)) {
    $copiedFiles += Copy-PinnedFile `
        -SourceRelative ([string]$reference.source) `
        -DestinationRelative ([string]$reference.destination) `
        -ExpectedSha256 ([string]$reference.sha256) `
        -Kind "reference"
}
foreach ($contract in @($production.contracts)) {
    $copiedFiles += Copy-PinnedFile `
        -SourceRelative ([string]$contract.source) `
        -DestinationRelative ([string]$contract.destination) `
        -Kind "contract"
}

$workspaceManifest = [ordered]@{
    schema_version = 1
    model_id = "aiko"
    workspace_layout_version = [int]$production.workspace_layout_version
    generated_by = "scripts/Prepare-AikoLive2DSource.ps1"
    production_manifest = "assets/live2d/aiko/source/production-manifest.json"
    production_manifest_sha256 = Get-Sha256 $ManifestPath
    stages = [ordered]@{
        layered_source = "not_delivered"
        cubism_project = "not_delivered"
        runtime_export = "not_delivered"
    }
    automation_limits = $production.automation_limits
    copied_files = $copiedFiles
}

$workspaceManifestPath = Join-Path $WorkspaceRoot "workspace-manifest.json"
$json = $workspaceManifest | ConvertTo-Json -Depth 20
[IO.File]::WriteAllText(
    $workspaceManifestPath,
    ($json.Replace("`r`n", "`n") + "`n"),
    [Text.UTF8Encoding]::new($false)
)

Write-Host "Aiko Live2D production workspace prepared."
Write-Host "Workspace: $WorkspaceRoot"
Write-Host "Copied references: $(@($production.references).Count)"
Write-Host "Copied contracts: $(@($production.contracts).Count)"
Write-Host "No PSD, CMO3, MOC3, MODEL3, layer separation, or rigging was generated."
