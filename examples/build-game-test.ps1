param(
    [string]$Destination = 'D:\CelsiusEngineGameTest'
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
    Copy-Item -LiteralPath (Join-Path $project 'README.md') -Destination $Destination

    $game = Join-Path $Destination 'baseplate.lua'
    if (-not (Test-Path -LiteralPath $game)) {
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'baseplate.lua') -Destination $game
    }
    Write-Output (Join-Path $Destination 'Jugar-Quest.cmd')
}
finally {
    Pop-Location
}
