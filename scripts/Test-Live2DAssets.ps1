# Validates Aiko Live2D specifications and optional source/ready/runtime assets.

[CmdletBinding()]
param(
    [string]$SourceRoot,
    [switch]$RequireSource,
    [switch]$RequireReady,
    [switch]$RequireRuntime,
    [switch]$RequireAll
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ([string]::IsNullOrWhiteSpace($SourceRoot)) {
    $SourceRoot = Split-Path -Parent $PSScriptRoot
}
$SourceRoot = [IO.Path]::GetFullPath($SourceRoot)
$Live2DRoot = Join-Path $SourceRoot "assets\live2d"
$AikoRoot = Join-Path $Live2DRoot "aiko"
$ManifestPath = Join-Path $AikoRoot "asset-manifest.json"

if ($RequireAll) {
    $RequireSource = $true
    $RequireReady = $true
    $RequireRuntime = $true
}

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

function Get-JsonProperty {
    param(
        [Parameter(Mandatory)]$Object,
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string]$Context
    )

    $property = $Object.PSObject.Properties[$Name]
    Assert-Condition ($null -ne $property) "$Context is missing property '$Name'."
    return $property.Value
}

function Assert-Columns {
    param(
        [Parameter(Mandatory)][object[]]$Rows,
        [Parameter(Mandatory)][string[]]$Columns,
        [Parameter(Mandatory)][string]$Path
    )

    Assert-Condition ($Rows.Count -gt 0) "CSV must contain at least one data row: $Path"
    $actual = @($Rows[0].PSObject.Properties.Name)
    Assert-Condition (($actual -join ",") -eq ($Columns -join ",")) `
        "CSV columns differ in '$Path'. Expected '$($Columns -join ",")', got '$($actual -join ",")'."
}

function Assert-UniqueValues {
    param(
        [Parameter(Mandatory)][object[]]$Rows,
        [Parameter(Mandatory)][string]$Column,
        [Parameter(Mandatory)][string]$Path
    )

    $values = @($Rows | ForEach-Object { [string]$_.$Column })
    $duplicates = @($values | Group-Object | Where-Object Count -gt 1 | ForEach-Object Name)
    Assert-Condition ($duplicates.Count -eq 0) `
        "CSV '$Path' contains duplicate $Column values: $($duplicates -join ", ")."
}

function Assert-BooleanText {
    param(
        [Parameter(Mandatory)][string]$Value,
        [Parameter(Mandatory)][string]$Context
    )

    Assert-Condition ($Value -in @("true", "false")) "$Context must be 'true' or 'false'."
}

function Resolve-AssetReference {
    param(
        [Parameter(Mandatory)][string]$Root,
        [Parameter(Mandatory)][string]$Reference,
        [Parameter(Mandatory)][string]$Context
    )

    Assert-Condition (-not [string]::IsNullOrWhiteSpace($Reference)) "$Context is empty."
    Assert-Condition (-not [IO.Path]::IsPathRooted($Reference)) `
        "$Context must be relative: $Reference"
    Assert-Condition (-not $Reference.Contains("\")) "$Context must use forward slashes: $Reference"

    $rootFull = [IO.Path]::GetFullPath($Root).TrimEnd("\", "/")
    $candidate = [IO.Path]::GetFullPath((Join-Path $rootFull $Reference.Replace("/", "\")))
    Assert-Condition ($candidate.StartsWith(
            "$rootFull\",
            [StringComparison]::OrdinalIgnoreCase
        )) "$Context escapes its stage root: $Reference"
    return $candidate
}

function Assert-Png {
    param([Parameter(Mandatory)][string]$Path)

    Assert-Condition (Test-Path -LiteralPath $Path -PathType Leaf) "PNG not found: $Path"
    $bytes = [IO.File]::ReadAllBytes($Path)
    $signature = [byte[]](137, 80, 78, 71, 13, 10, 26, 10)
    Assert-Condition ($bytes.Length -gt 33) "PNG is unexpectedly small: $Path"
    for ($index = 0; $index -lt $signature.Length; $index++) {
        Assert-Condition ($bytes[$index] -eq $signature[$index]) "Invalid PNG signature: $Path"
    }
    Assert-Condition ([Text.Encoding]::ASCII.GetString($bytes, 12, 4) -eq "IHDR") `
        "PNG is missing IHDR: $Path"
    $width = [Net.IPAddress]::NetworkToHostOrder([BitConverter]::ToInt32($bytes, 16))
    $height = [Net.IPAddress]::NetworkToHostOrder([BitConverter]::ToInt32($bytes, 20))
    $colorType = [int]$bytes[25]
    Assert-Condition ($width -gt 0 -and $height -gt 0) "PNG has invalid dimensions: $Path"
    Assert-Condition ($colorType -in @(4, 6)) `
        "PNG must use grayscale-alpha or RGBA color type for explicit transparency: $Path"
    return [pscustomobject]@{
        Width = $width
        Height = $height
    }
}

function Assert-Psd {
    param([Parameter(Mandatory)][string]$Path)

    Assert-Condition (Test-Path -LiteralPath $Path -PathType Leaf) "PSD not found: $Path"
    $bytes = [IO.File]::ReadAllBytes($Path)
    Assert-Condition ($bytes.Length -gt 26) "PSD is unexpectedly small: $Path"
    Assert-Condition ([Text.Encoding]::ASCII.GetString($bytes, 0, 4) -eq "8BPS") `
        "PSD does not have an 8BPS header: $Path"
    $height = [Net.IPAddress]::NetworkToHostOrder([BitConverter]::ToInt32($bytes, 14))
    $width = [Net.IPAddress]::NetworkToHostOrder([BitConverter]::ToInt32($bytes, 18))
    Assert-Condition ($width -gt 0 -and $height -gt 0) "PSD has invalid dimensions: $Path"
    return [pscustomobject]@{
        Width = $width
        Height = $height
    }
}

function Assert-Moc3 {
    param([Parameter(Mandatory)][string]$Path)

    Assert-Condition (Test-Path -LiteralPath $Path -PathType Leaf) "MOC3 not found: $Path"
    $bytes = [IO.File]::ReadAllBytes($Path)
    Assert-Condition ($bytes.Length -gt 64) "MOC3 is too small to be a runtime model: $Path"
    Assert-Condition ([Text.Encoding]::ASCII.GetString($bytes, 0, 4) -eq "MOC3") `
        "MOC3 does not have a Cubism MOC3 header: $Path"
}

function Assert-JsonVersion3 {
    param(
        [Parameter(Mandatory)]$Json,
        [Parameter(Mandatory)][string]$Context
    )

    $version = Get-JsonProperty -Object $Json -Name "Version" -Context $Context
    Assert-Condition ([int]$version -eq 3) "$Context Version must be 3."
}

function Assert-Motion3 {
    param([Parameter(Mandatory)][string]$Path)

    $motion = Read-JsonFile $Path
    Assert-JsonVersion3 -Json $motion -Context $Path
    $meta = Get-JsonProperty $motion "Meta" $Path
    $curves = @(Get-JsonProperty $motion "Curves" $Path)
    Assert-Condition ([double](Get-JsonProperty $meta "Duration" "$Path Meta") -gt 0) `
        "Motion duration must be positive: $Path"
    Assert-Condition ([double](Get-JsonProperty $meta "Fps" "$Path Meta") -ge 24) `
        "Motion FPS must be at least 24: $Path"
    Assert-Condition ((Get-JsonProperty $meta "Loop" "$Path Meta") -is [bool]) `
        "Motion Meta.Loop must be boolean: $Path"
    foreach ($name in @("CurveCount", "TotalSegmentCount", "TotalPointCount")) {
        Assert-Condition ([int](Get-JsonProperty $meta $name "$Path Meta") -gt 0) `
            "Motion Meta.$name must be positive: $Path"
    }
    Assert-Condition ($curves.Count -gt 0) "Motion Curves must not be empty: $Path"
    foreach ($curve in $curves) {
        $target = [string](Get-JsonProperty $curve "Target" "$Path curve")
        Assert-Condition ($target -in @("Model", "Parameter", "PartOpacity")) `
            "Unsupported motion curve target '$target': $Path"
        Assert-Condition (-not [string]::IsNullOrWhiteSpace(
                [string](Get-JsonProperty $curve "Id" "$Path curve")
            )) "Motion curve Id must not be empty: $Path"
        Assert-Condition (@(Get-JsonProperty $curve "Segments" "$Path curve").Count -ge 4) `
            "Motion curve Segments is too short: $Path"
    }
}

function Assert-Expression3 {
    param([Parameter(Mandatory)][string]$Path)

    $expression = Read-JsonFile $Path
    Assert-Condition (
        [string](Get-JsonProperty $expression "Type" $Path) -eq "Live2D Expression"
    ) "Expression Type must be 'Live2D Expression': $Path"
    $parameters = @(Get-JsonProperty $expression "Parameters" $Path)
    Assert-Condition ($parameters.Count -gt 0) "Expression Parameters must not be empty: $Path"
    foreach ($parameter in $parameters) {
        Assert-Condition (-not [string]::IsNullOrWhiteSpace(
                [string](Get-JsonProperty $parameter "Id" "$Path parameter")
            )) "Expression parameter Id must not be empty: $Path"
        [void][double](Get-JsonProperty $parameter "Value" "$Path parameter")
        $blend = [string](Get-JsonProperty $parameter "Blend" "$Path parameter")
        Assert-Condition ($blend -in @("Add", "Multiply", "Overwrite")) `
            "Unsupported expression blend '$blend': $Path"
    }
}

function Assert-Physics3 {
    param([Parameter(Mandatory)][string]$Path)

    $physics = Read-JsonFile $Path
    Assert-JsonVersion3 -Json $physics -Context $Path
    $meta = Get-JsonProperty $physics "Meta" $Path
    foreach ($name in @(
            "PhysicsSettingCount",
            "TotalInputCount",
            "TotalOutputCount",
            "VertexCount"
        )) {
        Assert-Condition ([int](Get-JsonProperty $meta $name "$Path Meta") -gt 0) `
            "Physics Meta.$name must be positive: $Path"
    }
    [void](Get-JsonProperty $meta "EffectiveForces" "$Path Meta")
    $settings = @(Get-JsonProperty $physics "PhysicsSettings" $Path)
    Assert-Condition ($settings.Count -gt 0) "PhysicsSettings must not be empty: $Path"
    foreach ($setting in $settings) {
        foreach ($name in @("Id", "Input", "Output", "Vertices", "Normalization")) {
            [void](Get-JsonProperty $setting $name "$Path physics setting")
        }
        Assert-Condition (@($setting.Input).Count -gt 0) "Physics input must not be empty: $Path"
        Assert-Condition (@($setting.Output).Count -gt 0) "Physics output must not be empty: $Path"
        Assert-Condition (@($setting.Vertices).Count -ge 2) `
            "Physics vertices must contain at least two points: $Path"
    }

    $outputIds = @(
        foreach ($setting in $settings) {
            foreach ($output in @($setting.Output)) {
                $destination = Get-JsonProperty $output "Destination" "$Path physics output"
                [string](Get-JsonProperty $destination "Id" "$Path physics output destination")
            }
        }
    )
    foreach ($requiredOutput in @(
            "ParamHairFront", "ParamHairSideL", "ParamHairSideR", "ParamHairBack",
            "ParamRibbonL", "ParamRibbonR"
        )) {
        Assert-Condition ($outputIds -contains $requiredOutput) `
            "Physics output is missing required parameter '$requiredOutput': $Path"
    }
}

function Get-StageStatus {
    param(
        [Parameter(Mandatory)]$Manifest,
        [Parameter(Mandatory)][string]$Name
    )

    $stages = Get-JsonProperty $Manifest "stages" "asset-manifest.json"
    $stage = Get-JsonProperty $stages $Name "asset-manifest.json stages"
    return [string](Get-JsonProperty $stage "status" "asset-manifest.json stage '$Name'")
}

Assert-Condition (Test-Path -LiteralPath $Live2DRoot -PathType Container) `
    "Live2D root not found: $Live2DRoot"
Assert-Condition (Test-Path -LiteralPath (Join-Path $SourceRoot "docs\LIVE2D_MODELING.md") -PathType Leaf) `
    "Live2D modeling document is missing."

$manifest = Read-JsonFile $ManifestPath
Assert-Condition ([int](Get-JsonProperty $manifest "schema_version" $ManifestPath) -eq 1) `
    "asset-manifest.json schema_version must be 1."
Assert-Condition ([string](Get-JsonProperty $manifest "model_id" $ManifestPath) -eq "aiko") `
    "asset-manifest.json model_id must be 'aiko'."

$allowedStatuses = @("missing", "spec_only", "in_progress", "ready")
foreach ($stageName in @("source", "ready", "runtime")) {
    $status = Get-StageStatus $manifest $stageName
    Assert-Condition ($status -in $allowedStatuses) `
        "Unsupported $stageName stage status '$status'."
}

$contractNames = @("model3", "motion3", "expression3", "physics3")
$contracts = Get-JsonProperty $manifest "contracts" $ManifestPath
foreach ($contractName in $contractNames) {
    $reference = [string](Get-JsonProperty $contracts $contractName "$ManifestPath contracts")
    $path = Resolve-AssetReference -Root $Live2DRoot -Reference $reference `
        -Context "$contractName contract"
    $schema = Read-JsonFile $path
    Assert-Condition (-not [string]::IsNullOrWhiteSpace(
            [string](Get-JsonProperty $schema '$schema' $path)
        )) "Schema declaration is missing: $path"
    Assert-Condition (-not [string]::IsNullOrWhiteSpace(
            [string](Get-JsonProperty $schema "title" $path)
        )) "Schema title is missing: $path"
}

$SourceStageRoot = Join-Path $AikoRoot "source"
$tableDefinitions = @(
    @{
        Name = "layers.csv"
        Columns = @(
            "sort", "layer_id", "psd_group", "png_file", "part", "side", "required",
            "clipping_parent", "note_zh"
        )
    },
    @{
        Name = "parameters.csv"
        Columns = @(
            "parameter_id", "min", "default", "max", "required", "driver", "purpose_zh"
        )
    },
    @{
        Name = "hit-areas.csv"
        Columns = @(
            "hit_area_name", "drawable_id", "source_layer_id", "interaction", "priority",
            "required", "notes_zh"
        )
    },
    @{
        Name = "motions.csv"
        Columns = @(
            "group", "name", "file", "fade_in_sec", "fade_out_sec", "loop", "trigger",
            "return_state", "required"
        )
    },
    @{
        Name = "expressions.csv"
        Columns = @(
            "name", "file", "fade_in_sec", "fade_out_sec", "required", "purpose_zh"
        )
    }
)

$tables = @{}
foreach ($definition in $tableDefinitions) {
    $path = Join-Path $SourceStageRoot $definition.Name
    Assert-Condition (Test-Path -LiteralPath $path -PathType Leaf) "Required table missing: $path"
    $rows = @(Import-Csv -LiteralPath $path -Encoding UTF8)
    Assert-Columns -Rows $rows -Columns $definition.Columns -Path $path
    $tables[$definition.Name] = $rows
}

$layers = @($tables["layers.csv"])
Assert-Condition ($layers.Count -ge 40) "layers.csv must define at least 40 separated layers."
Assert-UniqueValues $layers "layer_id" "layers.csv"
Assert-UniqueValues $layers "png_file" "layers.csv"
foreach ($row in $layers) {
    Assert-Condition ([string]$row.layer_id -match '^[a-z][a-z0-9_]*$') `
        "Invalid layer_id '$($row.layer_id)'."
    Assert-Condition ([string]$row.png_file -match '^png/parts/[a-z0-9_]+\.png$') `
        "Invalid PNG path '$($row.png_file)'."
    Assert-Condition (
        [IO.Path]::GetFileNameWithoutExtension([string]$row.png_file) -eq [string]$row.layer_id
    ) "Layer '$($row.layer_id)' must use a matching PNG file name."
    Assert-BooleanText ([string]$row.required) "layers.csv required for '$($row.layer_id)'"
    Assert-Condition ([string]$row.side -in @("left", "right", "center")) `
        "Invalid side '$($row.side)' for layer '$($row.layer_id)'."
}

$parameters = @($tables["parameters.csv"])
Assert-UniqueValues $parameters "parameter_id" "parameters.csv"
foreach ($row in $parameters) {
    Assert-Condition ([string]$row.parameter_id -match '^Param[A-Za-z0-9]+$') `
        "Invalid parameter_id '$($row.parameter_id)'."
    $minimum = [double]$row.min
    $default = [double]$row.default
    $maximum = [double]$row.max
    Assert-Condition ($minimum -le $default -and $default -le $maximum) `
        "Invalid range for parameter '$($row.parameter_id)'."
    Assert-BooleanText ([string]$row.required) "parameters.csv required for '$($row.parameter_id)'"
}
foreach ($requiredParameter in @(
        "ParamAngleX", "ParamAngleY", "ParamAngleZ", "ParamEyeLOpen", "ParamEyeROpen",
        "ParamEyeBallX", "ParamEyeBallY", "ParamMouthOpenY", "ParamMouthForm", "ParamBreath",
        "ParamAikoMicGlow", "ParamAikoThinking", "ParamAikoPet", "ParamAikoSleep"
    )) {
    Assert-Condition ($parameters.parameter_id -contains $requiredParameter) `
        "Required parameter is absent: $requiredParameter"
}

$hitAreas = @($tables["hit-areas.csv"])
Assert-UniqueValues $hitAreas "hit_area_name" "hit-areas.csv"
Assert-UniqueValues $hitAreas "drawable_id" "hit-areas.csv"
foreach ($requiredHitArea in @("Head", "Mic", "Body")) {
    Assert-Condition ($hitAreas.hit_area_name -contains $requiredHitArea) `
        "Required hit area is absent: $requiredHitArea"
}
foreach ($row in $hitAreas) {
    Assert-Condition ([string]$row.drawable_id -match '^D_Hit_[A-Za-z0-9]+$') `
        "Invalid hit-area drawable '$($row.drawable_id)'."
    Assert-Condition ($layers.layer_id -contains [string]$row.source_layer_id) `
        "Hit area '$($row.hit_area_name)' references unknown source layer '$($row.source_layer_id)'."
    Assert-BooleanText ([string]$row.required) "hit-areas.csv required for '$($row.hit_area_name)'"
}

$motions = @($tables["motions.csv"])
Assert-UniqueValues $motions "name" "motions.csv"
Assert-UniqueValues $motions "file" "motions.csv"
$requiredMotionGroups = @("idle", "listening", "processing", "success", "error", "petted", "sleepy")
foreach ($requiredGroup in $requiredMotionGroups) {
    $matching = @($motions | Where-Object {
            $_.group -eq $requiredGroup -and $_.required -eq "true"
        })
    Assert-Condition ($matching.Count -gt 0) "Required motion group is absent: $requiredGroup"
}
foreach ($row in $motions) {
    Assert-Condition ([string]$row.group -match '^[a-z][a-z0-9_]*$') `
        "Invalid motion group '$($row.group)'."
    Assert-Condition ([string]$row.file -match '^motions/[a-z0-9_]+\.motion3\.json$') `
        "Invalid motion path '$($row.file)'."
    Assert-Condition ([double]$row.fade_in_sec -ge 0 -and [double]$row.fade_out_sec -ge 0) `
        "Motion fade time must not be negative: $($row.name)"
    Assert-BooleanText ([string]$row.loop) "motions.csv loop for '$($row.name)'"
    Assert-BooleanText ([string]$row.required) "motions.csv required for '$($row.name)'"
}

$expressions = @($tables["expressions.csv"])
Assert-UniqueValues $expressions "name" "expressions.csv"
Assert-UniqueValues $expressions "file" "expressions.csv"
foreach ($requiredExpression in @(
        "neutral", "listening", "processing", "success", "error", "petted", "sleepy"
    )) {
    $matching = @($expressions | Where-Object {
            $_.name -eq $requiredExpression -and $_.required -eq "true"
        })
    Assert-Condition ($matching.Count -eq 1) "Required expression is absent: $requiredExpression"
}
foreach ($row in $expressions) {
    Assert-Condition ([string]$row.name -match '^[a-z][a-z0-9_]*$') `
        "Invalid expression name '$($row.name)'."
    Assert-Condition ([string]$row.file -match '^expressions/[a-z0-9_]+\.exp3\.json$') `
        "Invalid expression path '$($row.file)'."
    Assert-BooleanText ([string]$row.required) "expressions.csv required for '$($row.name)'"
}

$sourceStatus = Get-StageStatus $manifest "source"
$sourcePsd = Join-Path $SourceStageRoot "aiko_master.psd"
$sourceMustExist = $RequireSource -or $sourceStatus -eq "ready"
if ($sourceMustExist) {
    Assert-Condition ($sourceStatus -eq "ready") `
        "Source stage is required but manifest status is '$sourceStatus'; expected 'ready'."
    $psdInfo = Assert-Psd $sourcePsd
    foreach ($row in $layers | Where-Object required -eq "true") {
        $path = Resolve-AssetReference $SourceStageRoot ([string]$row.png_file) `
            "source layer '$($row.layer_id)'"
        $pngInfo = Assert-Png $path
        Assert-Condition (
            $pngInfo.Width -eq $psdInfo.Width -and $pngInfo.Height -eq $psdInfo.Height
        ) "Source PNG must preserve the PSD canvas dimensions: $path"
    }
}
else {
    $psdInfo = $null
    if (Test-Path -LiteralPath $sourcePsd -PathType Leaf) {
        $psdInfo = Assert-Psd $sourcePsd
    }
    foreach ($row in $layers) {
        $path = Resolve-AssetReference $SourceStageRoot ([string]$row.png_file) `
            "source layer '$($row.layer_id)'"
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            $pngInfo = Assert-Png $path
            if ($null -ne $psdInfo) {
                Assert-Condition (
                    $pngInfo.Width -eq $psdInfo.Width -and $pngInfo.Height -eq $psdInfo.Height
                ) "Source PNG must preserve the PSD canvas dimensions: $path"
            }
        }
    }
    Write-Warning "Aiko source stage is '$sourceStatus'; only specification tables were required."
}

$readyStatus = Get-StageStatus $manifest "ready"
$readyModel = Join-Path $AikoRoot "ready\aiko.cmo3"
$readyExists = Test-Path -LiteralPath $readyModel -PathType Leaf
$readyMustExist = $RequireReady -or $readyStatus -eq "ready" -or $readyExists
if ($readyMustExist) {
    if ($RequireReady) {
        Assert-Condition ($readyStatus -eq "ready") `
            "Ready stage is required but manifest status is '$readyStatus'; expected 'ready'."
    }
    Assert-Condition ($readyStatus -in @("in_progress", "ready")) `
        "aiko.cmo3 exists but ready stage status is '$readyStatus'."
    Assert-Condition (Test-Path -LiteralPath $readyModel -PathType Leaf) `
        "Cubism Editor model not found: $readyModel"
    Assert-Condition ((Get-Item -LiteralPath $readyModel).Length -gt 1024) `
        "aiko.cmo3 is unexpectedly small; do not use placeholder files."
}
else {
    Write-Warning "Aiko ready stage is '$readyStatus'; no Cubism Editor model was required."
}

$runtimeStatus = Get-StageStatus $manifest "runtime"
$runtimeRoot = Join-Path $AikoRoot "runtime"
$runtimeEntry = Join-Path $runtimeRoot "aiko.model3.json"
$runtimeExists = Test-Path -LiteralPath $runtimeEntry -PathType Leaf
$runtimeMustExist = $RequireRuntime -or $runtimeStatus -eq "ready" -or $runtimeExists
if ($runtimeMustExist) {
    if ($RequireRuntime) {
        Assert-Condition ($runtimeStatus -eq "ready") `
            "Runtime stage is required but manifest status is '$runtimeStatus'; expected 'ready'."
    }
    Assert-Condition ($runtimeStatus -in @("in_progress", "ready")) `
        "aiko.model3.json exists but runtime stage status is '$runtimeStatus'."
    $model = Read-JsonFile $runtimeEntry
    Assert-JsonVersion3 $model $runtimeEntry
    $references = Get-JsonProperty $model "FileReferences" $runtimeEntry

    $mocReference = [string](Get-JsonProperty $references "Moc" "$runtimeEntry FileReferences")
    Assert-Condition ($mocReference -match '^[a-z0-9_]+\.moc3$') `
        "Invalid MOC3 reference '$mocReference'."
    Assert-Moc3 (Resolve-AssetReference $runtimeRoot $mocReference "model3 Moc")

    $textureReferences = @(Get-JsonProperty $references "Textures" "$runtimeEntry FileReferences")
    Assert-Condition ($textureReferences.Count -gt 0) "model3 must reference at least one texture."
    foreach ($reference in $textureReferences) {
        Assert-Condition ([string]$reference -match '^textures/[a-z0-9_]+\.png$') `
            "Invalid texture reference '$reference'."
        [void](Assert-Png (
                Resolve-AssetReference $runtimeRoot ([string]$reference) "model3 texture"
            ))
    }

    $physicsReference = [string](Get-JsonProperty $references "Physics" "$runtimeEntry FileReferences")
    Assert-Condition ($physicsReference -match '^[a-z0-9_]+\.physics3\.json$') `
        "Invalid physics reference '$physicsReference'."
    Assert-Physics3 (Resolve-AssetReference $runtimeRoot $physicsReference "model3 physics")

    $modelMotions = Get-JsonProperty $references "Motions" "$runtimeEntry FileReferences"
    $motionFilesByGroup = @{}
    foreach ($requiredGroup in $requiredMotionGroups) {
        $entries = @(Get-JsonProperty $modelMotions $requiredGroup "$runtimeEntry Motions")
        Assert-Condition ($entries.Count -gt 0) "model3 motion group '$requiredGroup' is empty."
    }
    foreach ($groupProperty in $modelMotions.PSObject.Properties) {
        $motionFilesByGroup[$groupProperty.Name] = @()
        foreach ($entry in @($groupProperty.Value)) {
            $reference = [string](Get-JsonProperty $entry "File" "motion '$($groupProperty.Name)'")
            Assert-Condition ($reference -match '^motions/[a-z0-9_]+\.motion3\.json$') `
                "Invalid motion reference '$reference'."
            [void][double](Get-JsonProperty $entry "FadeInTime" "motion '$reference'")
            [void][double](Get-JsonProperty $entry "FadeOutTime" "motion '$reference'")
            Assert-Motion3 (Resolve-AssetReference $runtimeRoot $reference "model3 motion")
            $motionFilesByGroup[$groupProperty.Name] += $reference
        }
    }
    foreach ($requiredMotion in $motions | Where-Object required -eq "true") {
        Assert-Condition (
            $motionFilesByGroup.ContainsKey([string]$requiredMotion.group) -and
            $motionFilesByGroup[[string]$requiredMotion.group] -contains [string]$requiredMotion.file
        ) "model3 is missing required motion '$($requiredMotion.file)' in group '$($requiredMotion.group)'."
    }

    $modelExpressions = @(Get-JsonProperty $references "Expressions" "$runtimeEntry FileReferences")
    $expressionNames = @()
    foreach ($entry in $modelExpressions) {
        $name = [string](Get-JsonProperty $entry "Name" "model3 expression")
        $reference = [string](Get-JsonProperty $entry "File" "model3 expression '$name'")
        Assert-Condition ($reference -match '^expressions/[a-z0-9_]+\.exp3\.json$') `
            "Invalid expression reference '$reference'."
        Assert-Expression3 (Resolve-AssetReference $runtimeRoot $reference "model3 expression")
        $expressionNames += $name
    }
    foreach ($requiredExpression in $expressions | Where-Object required -eq "true") {
        Assert-Condition ($expressionNames -contains [string]$requiredExpression.name) `
            "model3 is missing expression '$($requiredExpression.name)'."
    }

    $groups = @(Get-JsonProperty $model "Groups" $runtimeEntry)
    $eyeBlinkGroups = @($groups | Where-Object Name -eq "EyeBlink")
    Assert-Condition ($eyeBlinkGroups.Count -eq 1) `
        "model3 must contain exactly one 'EyeBlink' parameter group."
    $eyeBlinkIds = @(Get-JsonProperty $eyeBlinkGroups[0] "Ids" "EyeBlink group")
    Assert-Condition (
        $eyeBlinkIds -contains "ParamEyeLOpen" -and $eyeBlinkIds -contains "ParamEyeROpen"
    ) "EyeBlink group must contain ParamEyeLOpen and ParamEyeROpen."
    $lipSyncGroups = @($groups | Where-Object Name -eq "LipSync")
    Assert-Condition ($lipSyncGroups.Count -eq 1) `
        "model3 must contain exactly one 'LipSync' parameter group."
    $lipSyncIds = @(Get-JsonProperty $lipSyncGroups[0] "Ids" "LipSync group")
    Assert-Condition ($lipSyncIds -contains "ParamMouthOpenY") `
        "LipSync group must contain ParamMouthOpenY."

    $modelHitAreas = @(Get-JsonProperty $model "HitAreas" $runtimeEntry)
    foreach ($requiredHitArea in $hitAreas | Where-Object required -eq "true") {
        $matches = @($modelHitAreas | Where-Object Name -eq $requiredHitArea.hit_area_name)
        Assert-Condition ($matches.Count -eq 1) `
            "model3 must contain exactly one '$($requiredHitArea.hit_area_name)' hit area."
        Assert-Condition (
            [string](Get-JsonProperty $matches[0] "Id" "model3 hit area") -eq
            [string]$requiredHitArea.drawable_id
        ) "model3 hit area '$($requiredHitArea.hit_area_name)' must use '$($requiredHitArea.drawable_id)'."
    }
}
else {
    Write-Warning "Aiko runtime stage is '$runtimeStatus'; no SDK runtime model was required."
}

Write-Host "Aiko Live2D asset validation passed."
Write-Host "Checked: manifest, JSON contracts, layer/parameter/hit-area/motion/expression tables."
Write-Host "Stage status: source=$sourceStatus, ready=$readyStatus, runtime=$runtimeStatus."
