$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskCargo = Join-Path $taskRoot '.tools\cargo'
if (Test-Path -LiteralPath (Join-Path $taskCargo 'bin\cargo.exe')) {
  $env:CARGO_HOME = $taskCargo
  $env:RUSTUP_HOME = Join-Path $taskRoot '.tools\rustup'
  $env:PATH = (Join-Path $taskCargo 'bin') + ';' + $env:PATH
}
$env:CARGO_BUILD_JOBS = '4'
$taskVs = Join-Path $taskRoot '.tools\vs\Common7\Tools\VsDevCmd.bat'
if (!(Test-Path -LiteralPath $taskVs)) {
  $taskVsWhere = 'C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe'
  if (Test-Path -LiteralPath $taskVsWhere) {
    $taskVsPath = & $taskVsWhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ($taskVsPath) { $taskVs = Join-Path $taskVsPath 'Common7\Tools\VsDevCmd.bat' }
  }
}
if (Test-Path -LiteralPath $taskVs) {
  # Import the compiler environment without printing any environment values.
  $taskEnvironmentCommand = 'call "' + $taskVs + '" -no_logo -arch=x64 -host_arch=x64 >nul && set'
  & $env:ComSpec /d /c $taskEnvironmentCommand | ForEach-Object {
    if ($_ -match '^([^=]+)=(.*)$') { [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process') }
  }
}
