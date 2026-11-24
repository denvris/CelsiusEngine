param(
    [string]$Destination = 'D:\CelsiusEngineGameTest',
    [switch]$DownloadAssets
)

$ErrorActionPreference = 'Stop'
$project = Split-Path -Parent $PSScriptRoot
$target = Join-Path $project 'target'
$Destination = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Destination)

Push-Location -LiteralPath $project
try {
    cargo build --locked --release --example baseplate --target-dir $target
    if ($LASTEXITCODE -ne 0) {
        throw "Cargo exited with code $LASTEXITCODE."
    }

    New-Item -ItemType Directory -Path $Destination -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $target 'release\examples\baseplate.exe') -Destination $Destination
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'launch-quest.cmd') -Destination (Join-Path $Destination 'Jugar-Quest.cmd')
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'launch-reflection-lab.cmd') -Destination (Join-Path $Destination 'Probar-Reflejos.cmd')
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'download-reflection-assets.ps1') -Destination $Destination
    Copy-Item -LiteralPath (Join-Path $project 'README.md') -Destination $Destination

    $assets = Join-Path $Destination 'assets\reflection-lab'
    $localAssets = Join-Path $project 'assets\reflection-lab'
    if (Test-Path -LiteralPath $localAssets -PathType Container) {
        New-Item -ItemType Directory -Path $assets -Force | Out-Null
        Get-ChildItem -LiteralPath $localAssets -File | Where-Object {
            $_.Extension -in '.jpg', '.hdr'
        } | Copy-Item -Destination $assets
    }
    if ($DownloadAssets) {
        & (Join-Path $PSScriptRoot 'download-reflection-assets.ps1') -Destination $assets
    }

    $game = Join-Path $Destination 'baseplate.lua'
    if (-not (Test-Path -LiteralPath $game)) {
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'baseplate.lua') -Destination $game
    }
    Write-Output (Join-Path $Destination 'Jugar-Quest.cmd')
    Write-Output (Join-Path $Destination 'Probar-Reflejos.cmd')
}
finally {
    Pop-Location
}
