# Validates a prepared Aiko Live2D modeler workspace and optional real deliveries.

[CmdletBinding()]
param(
    [string]$RepositoryRoot,
    [Parameter(Mandatory)]
    [string]$WorkspaceRoot,
    [switch]$RequireLayeredSource,
    [switch]$RequireCubismProject,
    [switch]$RequireRuntimeExport,
    [switch]$RequireAll
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) {
    $RepositoryRoot = Split-Path -Parent $PSScriptRoot
}
$RepositoryRoot = [IO.Path]::GetFullPath($RepositoryRoot)
$WorkspaceRoot = [IO.Path]::GetFullPath($WorkspaceRoot)
$ProductionManifestPath = Join-Path $RepositoryRoot `
    "assets\live2d\aiko\source\production-manifest.json"

if ($RequireAll) {
    $RequireLayeredSource = $true
    $RequireCubismProject = $true
    $RequireRuntimeExport = $true
}

function Assert-Condition {
    param(
        [Parameter(Mandatory)][bool]$Condition,
        [Parameter(Mandatory)][string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
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

function Get-Sha256 {
    param([Parameter(Mandatory)][string]$Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-PngInfo {
    param([Parameter(Mandatory)][string]$Path)

    Assert-Condition (Test-Path -LiteralPath $Path -PathType Leaf) "PNG not found: $Path"
    $bytes = [IO.File]::ReadAllBytes($Path)
    $signature = [byte[]](137, 80, 78, 71, 13, 10, 26, 10)
    Assert-Condition ($bytes.Length -gt 33) "PNG is unexpectedly small: $Path"
    for ($index = 0; $index -lt $signature.Length; $index++) {
        Assert-Condition ($bytes[$index] -eq $signature[$index]) "Invalid PNG signature: $Path"
    }
    $width = (
        [int64]$bytes[16] * 16777216 +
        [int64]$bytes[17] * 65536 +
        [int64]$bytes[18] * 256 +
        [int64]$bytes[19]
    )
    $height = (
        [int64]$bytes[20] * 16777216 +
        [int64]$bytes[21] * 65536 +
        [int64]$bytes[22] * 256 +
        [int64]$bytes[23]
    )
    return [pscustomobject]@{
        Width = $width
        Height = $height
        ColorType = [int]$bytes[25]
    }
}

function Assert-PsdEnvelope {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][int]$ExpectedWidth,
        [Parameter(Mandatory)][int]$ExpectedHeight
    )

    Assert-Condition (Test-Path -LiteralPath $Path -PathType Leaf) "Layered PSD not found: $Path"
    $bytes = [IO.File]::ReadAllBytes($Path)
    Assert-Condition ($bytes.Length -gt 26) "PSD is too small to be a real document: $Path"
    Assert-Condition ([Text.Encoding]::ASCII.GetString($bytes, 0, 4) -eq "8BPS") `
        "PSD does not have an 8BPS header: $Path"
    $height = [Net.IPAddress]::NetworkToHostOrder([BitConverter]::ToInt32($bytes, 14))
    $width = [Net.IPAddress]::NetworkToHostOrder([BitConverter]::ToInt32($bytes, 18))
    Assert-Condition ($width -eq $ExpectedWidth -and $height -eq $ExpectedHeight) `
        "PSD canvas must be ${ExpectedWidth}x${ExpectedHeight}: $Path"
}

function Assert-Cmo3Envelope {
    param([Parameter(Mandatory)][string]$Path)

    Assert-Condition (Test-Path -LiteralPath $Path -PathType Leaf) `
        "Cubism Editor project not found: $Path"
    $file = Get-Item -LiteralPath $Path
    Assert-Condition ($file.Length -gt 1024) `
        "CMO3 is too small to be a credible Cubism Editor project: $Path"
    if ($file.Length -le 4096) {
        $text = [Text.Encoding]::UTF8.GetString([IO.File]::ReadAllBytes($Path))
        Assert-Condition ($text -notmatch '(?i)\b(fake|dummy|placeholder|todo)\b') `
            "CMO3 contains obvious placeholder text: $Path"
    }
}

function Assert-Moc3Envelope {
    param([Parameter(Mandatory)][string]$Path)

    Assert-Condition (Test-Path -LiteralPath $Path -PathType Leaf) "MOC3 not found: $Path"
    $bytes = [IO.File]::ReadAllBytes($Path)
    Assert-Condition ($bytes.Length -gt 64) "MOC3 is too small: $Path"
    Assert-Condition ([Text.Encoding]::ASCII.GetString($bytes, 0, 4) -eq "MOC3") `
        "MOC3 does not have a Cubism MOC3 header: $Path"
}

function Assert-RuntimeReference {
    param(
        [Parameter(Mandatory)][string]$RuntimeRoot,
        [Parameter(Mandatory)][string]$Relative,
        [Parameter(Mandatory)][string]$Context
    )

    $path = Resolve-ContainedPath $RuntimeRoot $Relative $Context
    Assert-Condition (Test-Path -LiteralPath $path -PathType Leaf) `
        "$Context references a missing file: $Relative"
    Assert-Condition ((Get-Item -LiteralPath $path).Length -gt 0) `
        "$Context references an empty file: $Relative"
    return $path
}

Assert-Condition (Test-Path -LiteralPath $WorkspaceRoot -PathType Container) `
    "Workspace not found: $WorkspaceRoot"
$production = Read-JsonFile $ProductionManifestPath
$workspaceManifestPath = Join-Path $WorkspaceRoot "workspace-manifest.json"
$workspace = Read-JsonFile $workspaceManifestPath

Assert-Condition ([int]$workspace.schema_version -eq 1) `
    "workspace-manifest.json schema_version must be 1."
Assert-Condition ([string]$workspace.model_id -eq "aiko") `
    "workspace-manifest.json model_id must be 'aiko'."
Assert-Condition (
    [string]$workspace.production_manifest_sha256 -eq (Get-Sha256 $ProductionManifestPath)
) "Workspace was prepared from a different production-manifest.json."

$expectedCopied = @($production.references).Count + @($production.contracts).Count
Assert-Condition (@($workspace.copied_files).Count -eq $expectedCopied) `
    "workspace-manifest.json must list exactly $expectedCopied copied files."
foreach ($property in @(
        "creates_layered_psd",
        "creates_cmo3",
        "creates_moc3",
        "creates_model3",
        "performs_art_separation",
        "performs_rigging",
        "certifies_cubism_authenticity"
    )) {
    Assert-Condition (-not [bool]$workspace.automation_limits.$property) `
        "Workspace automation limit '$property' must remain false."
}

foreach ($reference in @($production.references)) {
    $source = Resolve-ContainedPath $RepositoryRoot ([string]$reference.source) "reference source"
    $target = Resolve-ContainedPath $WorkspaceRoot ([string]$reference.destination) `
        "reference destination"
    Assert-Condition (Test-Path -LiteralPath $source -PathType Leaf) `
        "Pinned reference source is missing: $($reference.source)"
    Assert-Condition (Test-Path -LiteralPath $target -PathType Leaf) `
        "Prepared reference is missing: $($reference.destination)"
    $expectedHash = ([string]$reference.sha256).ToLowerInvariant()
    Assert-Condition ((Get-Sha256 $source) -eq $expectedHash) `
        "Repository reference hash changed: $($reference.source)"
    Assert-Condition ((Get-Sha256 $target) -eq $expectedHash) `
        "Prepared reference differs from its pinned source: $($reference.destination)"
    $manifestEntries = @($workspace.copied_files | Where-Object {
            [string]$_.path -eq [string]$reference.destination
        })
    Assert-Condition ($manifestEntries.Count -eq 1) `
        "Workspace manifest must contain one entry for $($reference.destination)."
    Assert-Condition (
        [string]$manifestEntries[0].kind -eq "reference" -and
        [string]$manifestEntries[0].source -eq [string]$reference.source -and
        [string]$manifestEntries[0].sha256 -eq $expectedHash
    ) "Workspace manifest reference entry differs: $($reference.destination)"
    $png = Get-PngInfo $target
    Assert-Condition (
        $png.Width -eq [int]$reference.width -and
        $png.Height -eq [int]$reference.height
    ) "Prepared reference dimensions differ: $($reference.destination)"
}

foreach ($contract in @($production.contracts)) {
    $source = Resolve-ContainedPath $RepositoryRoot ([string]$contract.source) "contract source"
    $target = Resolve-ContainedPath $WorkspaceRoot ([string]$contract.destination) `
        "contract destination"
    Assert-Condition (Test-Path -LiteralPath $source -PathType Leaf) `
        "Contract source is missing: $($contract.source)"
    Assert-Condition (Test-Path -LiteralPath $target -PathType Leaf) `
        "Prepared contract is missing: $($contract.destination)"
    $contractHash = Get-Sha256 $source
    Assert-Condition ($contractHash -eq (Get-Sha256 $target)) `
        "Prepared contract differs from its repository source: $($contract.destination)"
    $manifestEntries = @($workspace.copied_files | Where-Object {
            [string]$_.path -eq [string]$contract.destination
        })
    Assert-Condition ($manifestEntries.Count -eq 1) `
        "Workspace manifest must contain one entry for $($contract.destination)."
    Assert-Condition (
        [string]$manifestEntries[0].kind -eq "contract" -and
        [string]$manifestEntries[0].source -eq [string]$contract.source -and
        [string]$manifestEntries[0].sha256 -eq $contractHash
    ) "Workspace manifest contract entry differs: $($contract.destination)"
}

$artifactFiles = @(
    Get-ChildItem -LiteralPath (Join-Path $WorkspaceRoot "handoff") -File -Recurse |
        Where-Object { $_.Name -match '\.(psd|cmo3|moc3)$' }
)
foreach ($artifact in $artifactFiles) {
    Assert-Condition ($artifact.Length -gt 0) "Empty placeholder artifact is forbidden: $($artifact.FullName)"
}

if ($RequireLayeredSource) {
    $sourceRoot = Join-Path $WorkspaceRoot "handoff\source"
    Assert-PsdEnvelope `
        -Path (Join-Path $sourceRoot "aiko_master.psd") `
        -ExpectedWidth ([int]$production.canvas.width) `
        -ExpectedHeight ([int]$production.canvas.height)

    $layers = @(Import-Csv -LiteralPath (Join-Path $WorkspaceRoot "contracts\layers.csv") -Encoding UTF8)
    foreach ($layer in $layers | Where-Object required -eq "true") {
        $partPath = Resolve-ContainedPath $sourceRoot ([string]$layer.png_file) `
            "required layer '$($layer.layer_id)'"
        $png = Get-PngInfo $partPath
        Assert-Condition (
            $png.Width -eq [int]$production.canvas.width -and
            $png.Height -eq [int]$production.canvas.height
        ) "Layer PNG must preserve the full canvas: $partPath"
        Assert-Condition ($png.ColorType -in @(4, 6)) `
            "Layer PNG must contain an alpha channel: $partPath"
    }
}

if ($RequireCubismProject) {
    Assert-Cmo3Envelope (Join-Path $WorkspaceRoot "handoff\ready\aiko.cmo3")
}

if ($RequireRuntimeExport) {
    $runtimeRoot = Join-Path $WorkspaceRoot "handoff\runtime"
    $modelPath = Join-Path $runtimeRoot "aiko.model3.json"
    $model = Read-JsonFile $modelPath
    Assert-Condition ([int]$model.Version -ge 3) "aiko.model3.json Version must be at least 3."
    $references = $model.FileReferences
    Assert-Condition ($null -ne $references) "aiko.model3.json is missing FileReferences."

    $mocRelative = [string]$references.Moc
    Assert-Condition ($mocRelative.EndsWith(".moc3", [StringComparison]::OrdinalIgnoreCase)) `
        "FileReferences.Moc must reference a .moc3 file."
    $mocPath = Assert-RuntimeReference $runtimeRoot $mocRelative "FileReferences.Moc"
    Assert-Moc3Envelope $mocPath

    $textures = @($references.Textures)
    Assert-Condition ($textures.Count -gt 0) "Runtime export must contain at least one texture."
    foreach ($texture in $textures) {
        $texturePath = Assert-RuntimeReference $runtimeRoot ([string]$texture) "texture"
        [void](Get-PngInfo $texturePath)
    }
    foreach ($entry in @($references.Expressions)) {
        [void](Assert-RuntimeReference $runtimeRoot ([string]$entry.File) "expression")
    }
    foreach ($group in @($references.Motions.PSObject.Properties)) {
        foreach ($motion in @($group.Value)) {
            [void](Assert-RuntimeReference $runtimeRoot ([string]$motion.File) `
                "motion group '$($group.Name)'")
        }
    }

    $sourceArtifacts = @(
        Get-ChildItem -LiteralPath $runtimeRoot -File -Recurse |
            Where-Object { $_.Name -match '\.(psd|cmo3)$' }
    )
    Assert-Condition ($sourceArtifacts.Count -eq 0) `
        "Runtime export must not contain PSD or CMO3 source files."
}

Write-Host "Aiko Live2D source workspace contracts passed."
Write-Host "This check does not certify PSD layer quality or Cubism project authenticity."
Write-Host "Cubism Editor review and target-renderer load testing remain mandatory."
