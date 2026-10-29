$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'dev-env.ps1')
Set-Location (Split-Path -Parent $PSScriptRoot)
cargo build --release -p codebook-native-host
if ($LASTEXITCODE -ne 0) { throw 'Native messaging host build failed' }
npm.cmd run tauri -- dev
if ($LASTEXITCODE -ne 0) { throw 'Codebook development server exited with an error' }
