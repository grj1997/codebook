$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'dev-env.ps1')
Set-Location (Split-Path -Parent $PSScriptRoot)
npm.cmd run check
if ($LASTEXITCODE -ne 0) { throw 'Frontend or extension checks failed' }
cargo test -p codebook-core --features native
if ($LASTEXITCODE -ne 0) { throw 'Core tests failed' }
cargo build --release -p codebook-native-host
if ($LASTEXITCODE -ne 0) { throw 'Native messaging host build failed' }
npm.cmd run tauri -- build
if ($LASTEXITCODE -ne 0) { throw 'Desktop application build failed' }
