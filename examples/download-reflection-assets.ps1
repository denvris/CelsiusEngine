param(
    [string]$Destination
)

$ErrorActionPreference = 'Stop'
if (-not $Destination) {
    $root = if (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'baseplate.exe')) {
        $PSScriptRoot
    } else {
        Split-Path -Parent $PSScriptRoot
    }
    $Destination = Join-Path $root 'assets\reflection-lab'
}
$cdn = 'https://dl.polyhaven.org/file/ph-assets'
$files = [ordered]@{
    'concrete_floor_diff_4k.jpg' = 'Textures/jpg/4k/concrete_floor/concrete_floor_diff_4k.jpg'
    'concrete_floor_rough_4k.jpg' = 'Textures/jpg/4k/concrete_floor/concrete_floor_rough_4k.jpg'
    'wood_table_001_diff_4k.jpg' = 'Textures/jpg/4k/wood_table_001/wood_table_001_diff_4k.jpg'
    'wood_table_001_rough_4k.jpg' = 'Textures/jpg/4k/wood_table_001/wood_table_001_rough_4k.jpg'
    'kloppenheim_05_puresky_4k.hdr' = 'HDRIs/hdr/4k/kloppenheim_05_puresky_4k.hdr'
}

New-Item -ItemType Directory -Path $Destination -Force | Out-Null
foreach ($asset in $files.GetEnumerator()) {
    $path = Join-Path $Destination $asset.Key
    if (Test-Path -LiteralPath $path -PathType Leaf) { continue }
    $download = "$path.part"
    try {
        Invoke-WebRequest -UseBasicParsing -Uri "$cdn/$($asset.Value)" -OutFile $download -TimeoutSec 120
        Move-Item -LiteralPath $download -Destination $path
    } finally {
        if (Test-Path -LiteralPath $download) {
            Remove-Item -LiteralPath $download
        }
    }
}
Write-Output (Get-Item -LiteralPath $Destination).FullName
