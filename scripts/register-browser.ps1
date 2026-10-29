param([ValidateSet('debug','release')] [string] $Configuration = 'debug')
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'dev-env.ps1')
$taskRoot = Split-Path -Parent $PSScriptRoot
Set-Location $taskRoot
cargo build -p codebook-native-host $(if ($Configuration -eq 'release') { '--release' })
if ($LASTEXITCODE -ne 0) { throw 'Native host build failed' }
$taskDirectory = Join-Path $taskRoot "target\$Configuration"
& (Join-Path $taskDirectory 'codebook-native-host.exe') --register $taskDirectory
if ($LASTEXITCODE -ne 0) { throw 'Native host registration failed' }
Write-Output 'Browser host registered for this Windows user. Load browser-extension/dist in Chrome or Edge, then pair it in Codebook settings.'
