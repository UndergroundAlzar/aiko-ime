[CmdletBinding()]
param(
    [string]$SourceRoot = (Join-Path $PSScriptRoot "..")
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$RepositoryRoot = [IO.Path]::GetFullPath($SourceRoot)
$CagRoot = Join-Path $RepositoryRoot "assets\live2d\aiko_cag"
$ContractRoot = Join-Path $CagRoot "source"
$ChecklistPath = Join-Path $RepositoryRoot "docs\CAG_MODELING_CHECKLIST.md"

function Assert-Condition {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) {
        throw $Message
    }
}

function Read-JsonFile {
    param([string]$Path)
    Assert-Condition (Test-Path -LiteralPath $Path -PathType Leaf) "JSON file missing: $Path"
    try {
        return Get-Content -LiteralPath $Path -Raw -Encoding UTF8 | ConvertFrom-Json
    }
    catch {
        throw "Invalid JSON file '$Path': $($_.Exception.Message)"
    }
}

function Assert-ExactColumns {
    param([object[]]$Rows, [string[]]$Columns, [string]$Path)
    Assert-Condition ($Rows.Count -gt 0) "CSV has no data rows: $Path"
    $actual = @($Rows[0].PSObject.Properties.Name)
    Assert-Condition (($actual -join ",") -eq ($Columns -join ",")) `
        "Unexpected CSV columns in '$Path'. Expected '$($Columns -join ',')'; got '$($actual -join ',')'."
}

function Assert-Unique {
    param([object[]]$Rows, [string]$Column, [string]$Path)
    $duplicates = @(
        $Rows |
            Group-Object -Property $Column |
            Where-Object { $_.Count -gt 1 -or [string]::IsNullOrWhiteSpace([string]$_.Name) }
    )
    $duplicateNames = @($duplicates | ForEach-Object { [string]$_.Name })
    Assert-Condition ($duplicates.Count -eq 0) `
        "CSV '$Path' contains duplicate or empty '$Column' values: $($duplicateNames -join ', ')."
}

function Assert-BooleanText {
    param([string]$Value, [string]$Context)
    Assert-Condition ($Value -in @("true", "false")) "Expected true/false for $Context; got '$Value'."
}

function Get-PngInfo {
    param([string]$Path)
    $bytes = [IO.File]::ReadAllBytes($Path)
    Assert-Condition ($bytes.Length -ge 26) "PNG is too small: $Path"
    $signature = [byte[]](137, 80, 78, 71, 13, 10, 26, 10)
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
        Width = [int64]$width
        Height = [int64]$height
        ColorType = [int]$bytes[25]
    }
}

function Resolve-ReferencePath {
    param([string]$BasePath, [string]$Reference)
    Assert-Condition (-not [IO.Path]::IsPathRooted($Reference)) `
        "Reference path must be relative: $Reference"
    $resolved = [IO.Path]::GetFullPath((Join-Path $BasePath $Reference))
    $rootPrefix = $RepositoryRoot.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    Assert-Condition ($resolved.StartsWith($rootPrefix, [StringComparison]::OrdinalIgnoreCase)) `
        "Reference escapes the repository: $Reference"
    return $resolved
}

function Get-RowById {
    param([object[]]$Rows, [string]$Column, [string]$Value, [string]$Context)
    $matches = @($Rows | Where-Object { [string]$_.$Column -eq $Value })
    Assert-Condition ($matches.Count -eq 1) "Expected one $Context '$Value'; found $($matches.Count)."
    return $matches[0]
}

Assert-Condition (Test-Path -LiteralPath $CagRoot -PathType Container) `
    "CAG asset root missing: $CagRoot"
Assert-Condition (Test-Path -LiteralPath $ChecklistPath -PathType Leaf) `
    "CAG modeling checklist missing: $ChecklistPath"

$manifestPath = Join-Path $CagRoot "asset-manifest.json"
$manifest = Read-JsonFile $manifestPath
Assert-Condition ([int]$manifest.schema_version -eq 2) "CAG manifest schema_version must be 2."
Assert-Condition ([string]$manifest.model_id -eq "aiko_cag") "CAG manifest model_id must be aiko_cag."
Assert-Condition ([string]$manifest.stages.source.status -eq "concept_only") `
    "CAG source stage must remain concept_only until a real layered PSD is delivered."
Assert-Condition ([string]$manifest.stages.ready.status -eq "missing") `
    "CAG ready stage must remain missing until a real Cubism Editor project is delivered."
Assert-Condition ([string]$manifest.stages.runtime.status -eq "missing") `
    "CAG runtime stage must remain missing until an official Cubism export is delivered."
Assert-Condition ([bool]$manifest.fixed_builtin_model) "CAG model must remain fixed_builtin_model=true."
Assert-Condition (-not [bool]$manifest.allow_arbitrary_model_import) `
    "CAG model must keep allow_arbitrary_model_import=false."
Assert-Condition (-not [bool]$manifest.placeholder_policy.allow_fake_moc3) `
    "Fake moc3 files must remain forbidden."
Assert-Condition (-not [bool]$manifest.placeholder_policy.allow_fake_cmo3) `
    "Fake cmo3 files must remain forbidden."
Assert-Condition (-not [bool]$manifest.placeholder_policy.allow_handwritten_model3) `
    "Handwritten model3 files must remain forbidden."
Assert-Condition ([int]$manifest.invariants.gpnvg_tube_count -eq 4) `
    "Manifest must require four GPNVG tubes."
Assert-Condition ([string]$manifest.invariants.glove_coverage -eq "full_finger") `
    "Manifest must require full-finger gloves."
Assert-Condition ([string]$manifest.invariants.weapon_model -eq "KAC KS-1") `
    "Manifest weapon model must remain KAC KS-1."
Assert-Condition (
    [string]$manifest.invariants.weapon_carry -eq "chest_slung" -and
    [string]$manifest.invariants.weapon_muzzle -eq "down"
) "Manifest weapon carry must remain chest-slung and muzzle-down."
$manifestPatches = @($manifest.invariants.chest_patches | ForEach-Object { [string]$_ })
Assert-Condition (($manifestPatches -join "|") -eq "AIKO|B RH-") `
    "Manifest chest patches must be exactly AIKO and B RH-."
Assert-Condition ([bool]$manifest.invariants.allow_equipment_boom_mic) `
    "Manifest must distinguish the allowed equipment boom mic."
Assert-Condition (-not [bool]$manifest.invariants.allow_microphone_icon_or_patch) `
    "Manifest must forbid microphone icons and patches."

$expectedContracts = @(
    "source/reference.json",
    "source/layers.csv",
    "source/layers-addon.csv",
    "source/parameters.csv",
    "source/motions.csv",
    "source/expressions.csv",
    "source/occlusion-redraw.csv"
)
$manifestContracts = @($manifest.stages.source.contract_files | ForEach-Object { [string]$_ })
Assert-Condition (($manifestContracts -join "|") -eq ($expectedContracts -join "|")) `
    "Manifest contract_files must match the authoritative CAG contract list."
foreach ($relative in $expectedContracts) {
    Assert-Condition (Test-Path -LiteralPath (Join-Path $CagRoot $relative) -PathType Leaf) `
        "Manifest contract file missing: $relative"
}

$referencePath = Join-Path $ContractRoot "reference.json"
$reference = Read-JsonFile $referencePath
Assert-Condition ([int]$reference.canvas.width -eq 1024 -and [int]$reference.canvas.height -eq 1536) `
    "CAG reference canvas must remain 1024 x 1536."
foreach ($name in @("master", "cutout", "expression_sheet", "motion_sheet", "parts_reference")) {
    $entry = $reference.$name
    $path = Resolve-ReferencePath $ContractRoot ([string]$entry.path)
    Assert-Condition (Test-Path -LiteralPath $path -PathType Leaf) "Reference PNG missing: $path"
    $png = Get-PngInfo $path
    $hasWidth = $entry.PSObject.Properties.Name -contains "width"
    $hasHeight = $entry.PSObject.Properties.Name -contains "height"
    $expectedWidth = if ($hasWidth) {
        [int]$entry.width
    }
    else {
        [int]$reference.canvas.width
    }
    $expectedHeight = if ($hasHeight) {
        [int]$entry.height
    }
    else {
        [int]$reference.canvas.height
    }
    Assert-Condition (
        $png.Width -eq $expectedWidth -and
        $png.Height -eq $expectedHeight
    ) "Reference PNG dimensions do not match reference.json: $path"
    Assert-Condition ($png.ColorType -eq [int]$entry.png_color_type) `
        "Reference PNG color type does not match reference.json: $path"
    $hash = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
    Assert-Condition ($hash -eq ([string]$entry.sha256).ToLowerInvariant()) `
        "Reference PNG SHA-256 does not match reference.json: $path"
}

$definitions = @(
    @{
        Name = "layers.csv"
        Key = "layer_id"
        Columns = @(
            "sort", "layer_id", "psd_group", "png_file", "part", "side", "required",
            "physics", "occlusion_role", "visual_text", "contract_tags", "note_zh"
        )
    },
    @{
        Name = "layers-addon.csv"
        Key = "layer_id"
        Columns = @(
            "sort", "layer_id", "psd_group", "png_file", "part", "side", "required",
            "physics", "occlusion_role", "visual_text", "contract_tags", "note_zh"
        )
    },
    @{
        Name = "parameters.csv"
        Key = "parameter_id"
        Columns = @(
            "parameter_id", "min", "default", "max", "required", "driver", "bind_layers",
            "constraint_zh", "purpose_zh"
        )
    },
    @{
        Name = "motions.csv"
        Key = "name"
        Columns = @(
            "group", "name", "file", "fade_in_sec", "fade_out_sec", "loop", "trigger",
            "return_state", "required", "hand_pose", "weapon_pose", "gpnvg_pose", "acceptance_zh"
        )
    },
    @{
        Name = "expressions.csv"
        Key = "name"
        Columns = @(
            "name", "file", "fade_in_sec", "fade_out_sec", "required", "uses_icon",
            "patch_visibility", "purpose_zh"
        )
    },
    @{
        Name = "occlusion-redraw.csv"
        Key = "redraw_id"
        Columns = @(
            "redraw_id", "back_layer_id", "occluder_layer_ids", "region", "required",
            "deliverable", "acceptance_zh"
        )
    }
)

$tables = @{}
foreach ($definition in $definitions) {
    $path = Join-Path $ContractRoot $definition.Name
    $rows = @(Import-Csv -LiteralPath $path -Encoding UTF8)
    Assert-ExactColumns $rows $definition.Columns $path
    Assert-Unique $rows $definition.Key $path
    $tables[$definition.Name] = $rows
}

$layers = @($tables["layers.csv"])
$addon = @($tables["layers-addon.csv"])
Assert-Condition ($layers.Count -ge 70) "layers.csv must define at least 70 complete model layers."
Assert-Condition ($addon.Count -ge 35) "layers-addon.csv must define the complete tactical subset."
Assert-Unique $layers "png_file" "layers.csv"
foreach ($row in $layers) {
    Assert-Condition ([string]$row.layer_id -match '^[a-z][a-z0-9_]*$') `
        "Invalid layer_id '$($row.layer_id)'."
    Assert-Condition ([string]$row.png_file -eq "png/parts/$($row.layer_id).png") `
        "Layer '$($row.layer_id)' must use a matching full-canvas PNG path."
    Assert-Condition ([string]$row.side -in @("left", "right", "center")) `
        "Invalid side '$($row.side)' for '$($row.layer_id)'."
    Assert-BooleanText ([string]$row.required) "layers.csv required for '$($row.layer_id)'"
    Assert-BooleanText ([string]$row.physics) "layers.csv physics for '$($row.layer_id)'"
}

foreach ($addonRow in $addon) {
    $fullRow = Get-RowById $layers "layer_id" ([string]$addonRow.layer_id) "full layer"
    foreach ($column in $definitions[0].Columns) {
        Assert-Condition ([string]$addonRow.$column -eq [string]$fullRow.$column) `
            "Tactical addendum differs from layers.csv for '$($addonRow.layer_id)' column '$column'."
    }
}

$gpnvgTubes = @($layers | Where-Object { $_.part -eq "gpnvg_tube" -and $_.required -eq "true" })
Assert-Condition ($gpnvgTubes.Count -eq 4) "Exactly four required GPNVG tube layers are required."
foreach ($id in @(
        "gpnvg_tube_outer_l", "gpnvg_tube_inner_l",
        "gpnvg_tube_inner_r", "gpnvg_tube_outer_r"
    )) {
    $tube = Get-RowById $gpnvgTubes "layer_id" $id "GPNVG tube"
    Assert-Condition ([string]$tube.contract_tags -match '(^|;)four_tube(;|$)') `
        "GPNVG tube '$id' must carry the four_tube contract tag."
}

foreach ($id in @("glove_l", "glove_r", "glove_fingers_l", "glove_fingers_r")) {
    $glove = Get-RowById $layers "layer_id" $id "glove layer"
    Assert-Condition ([string]$glove.contract_tags -match '(^|;)full_finger(;|$)') `
        "Glove layer '$id' must be full_finger."
}

$weapon = Get-RowById $layers "layer_id" "ks1_rifle" "weapon layer"
foreach ($tag in @("KAC_KS-1", "chest_slung", "muzzle_down", "hands_clear")) {
    Assert-Condition (([string]$weapon.contract_tags -split ';') -contains $tag) `
        "ks1_rifle is missing contract tag '$tag'."
}

$patchRows = @($layers | Where-Object part -eq "patch")
Assert-Condition ($patchRows.Count -eq 2) "Exactly two chest patch layers are allowed."
$patchTexts = @($patchRows.visual_text | Sort-Object)
$expectedPatchTexts = @("AIKO", "B RH-") | Sort-Object
Assert-Condition (($patchTexts -join "|") -eq ($expectedPatchTexts -join "|")) `
    "Chest patch text must be exactly AIKO and B RH-."
$forbiddenVisual = @("mic_icon", "microphone_icon", "mic_patch", "microphone_patch", "麦克风图标", "麦克风贴章")
foreach ($row in $layers) {
    $surface = "$($row.layer_id)|$($row.visual_text)".ToLowerInvariant()
    foreach ($token in $forbiddenVisual) {
        Assert-Condition (-not $surface.Contains($token.ToLowerInvariant())) `
            "Forbidden microphone icon or patch token '$token' found in layer '$($row.layer_id)'."
    }
}

$parameters = @($tables["parameters.csv"])
foreach ($row in $parameters) {
    Assert-Condition ([double]$row.min -le [double]$row.default -and [double]$row.default -le [double]$row.max) `
        "Invalid parameter range for '$($row.parameter_id)'."
    Assert-BooleanText ([string]$row.required) "parameters.csv required for '$($row.parameter_id)'"
}
foreach ($id in @(
        "ParamCagGpnvgDeploy", "ParamCagSlingL", "ParamCagSlingR",
        "ParamCagRifleX", "ParamCagRifleY", "ParamCagRadioCable",
        "ParamAikoListening"
    )) {
    [void](Get-RowById $parameters "parameter_id" $id "required CAG parameter")
}
Assert-Condition (-not ($parameters.parameter_id -contains "ParamAikoMicGlow")) `
    "CAG must not define ParamAikoMicGlow because microphone icon/glow branding is forbidden."

$motions = @($tables["motions.csv"])
foreach ($row in $motions) {
    Assert-BooleanText ([string]$row.loop) "motions.csv loop for '$($row.name)'"
    Assert-BooleanText ([string]$row.required) "motions.csv required for '$($row.name)'"
    Assert-Condition ([string]$row.hand_pose -match '_clear$') `
        "Motion '$($row.name)' must keep hands clear of the weapon."
    Assert-Condition ([string]$row.weapon_pose -eq "chest_slung_muzzle_down") `
        "Motion '$($row.name)' must keep the rifle chest-slung and muzzle-down."
}
foreach ($group in @("idle", "listening", "processing", "success", "error", "petted", "sleepy", "wave")) {
    Assert-Condition (@($motions | Where-Object { $_.group -eq $group -and $_.required -eq "true" }).Count -eq 1) `
        "Required motion group missing or duplicated: $group"
}

$expressions = @($tables["expressions.csv"])
foreach ($row in $expressions) {
    Assert-BooleanText ([string]$row.required) "expressions.csv required for '$($row.name)'"
    Assert-BooleanText ([string]$row.uses_icon) "expressions.csv uses_icon for '$($row.name)'"
    Assert-Condition ([string]$row.uses_icon -eq "false") `
        "Expression '$($row.name)' must not display an icon."
    Assert-Condition ([string]$row.patch_visibility -eq "clear") `
        "Expression '$($row.name)' must keep both chest patches clear."
}
foreach ($name in @("neutral", "listening", "focused", "success", "error", "petted", "sleepy")) {
    Assert-Condition (@($expressions | Where-Object { $_.name -eq $name -and $_.required -eq "true" }).Count -eq 1) `
        "Required expression missing or duplicated: $name"
}

$redraw = @($tables["occlusion-redraw.csv"])
Assert-Condition ($redraw.Count -ge 15) "occlusion-redraw.csv must cover all major hidden regions."
foreach ($row in $redraw) {
    Assert-BooleanText ([string]$row.required) "occlusion-redraw.csv required for '$($row.redraw_id)'"
    Assert-Condition ($layers.layer_id -contains [string]$row.back_layer_id) `
        "Redraw '$($row.redraw_id)' references unknown back layer '$($row.back_layer_id)'."
    foreach ($occluder in ([string]$row.occluder_layer_ids -split ';')) {
        Assert-Condition ($layers.layer_id -contains $occluder) `
            "Redraw '$($row.redraw_id)' references unknown occluder '$occluder'."
    }
}

$checklist = Get-Content -LiteralPath $ChecklistPath -Raw -Encoding UTF8
foreach ($needle in @(
        "source/reference.json", "source/layers.csv", "source/layers-addon.csv",
        "source/parameters.csv", "source/motions.csv", "source/expressions.csv",
        "source/occlusion-redraw.csv", "四个独立镜筒", "全指手套", "KAC KS-1",
        "枪口始终朝下", "AIKO", "B RH-", "禁止麦克风图标"
    )) {
    Assert-Condition ($checklist.Contains($needle)) "Checklist is missing required contract text: $needle"
}

$forbiddenFiles = @(
    Get-ChildItem -LiteralPath $CagRoot -Recurse -File |
        Where-Object {
            $_.Name -match '\.(cmo3|moc3)$' -or
            $_.Name -match '\.model3\.json$' -or
            $_.Extension -ieq ".psd"
        }
)
$forbiddenPaths = @($forbiddenFiles | ForEach-Object { [string]$_.FullName })
Assert-Condition ($forbiddenFiles.Count -eq 0) `
    "concept_only CAG assets must not contain PSD/Cubism placeholders: $($forbiddenPaths -join ', ')"

Write-Host "Aiko CAG art asset contracts passed."
