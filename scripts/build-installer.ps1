# Builds veil.exe in release mode and packages it into dist\VeilSetup-<version>.exe.
# Needs: rustup (GNU host), WinLibs MinGW (winget BrechtSanders.WinLibs.POSIX.UCRT), Inno Setup 6.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent

$mingw = Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages" -Directory -Filter 'BrechtSanders.WinLibs*' |
    ForEach-Object { Join-Path $_.FullName 'mingw64\bin' } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $mingw) { throw 'WinLibs MinGW not found: winget install BrechtSanders.WinLibs.POSIX.UCRT' }
$env:Path = "$env:USERPROFILE\.cargo\bin;$mingw;$env:Path"

Push-Location $root
try {
    cargo test --release
    if ($LASTEXITCODE) { throw 'tests failed' }
    cargo build --release
    if ($LASTEXITCODE) { throw 'build failed' }

    $loader = Get-ChildItem 'target\release\build' -Recurse -Filter WebView2Loader.dll |
        Where-Object { $_.Directory.Name -eq 'x64' } | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    Copy-Item $loader.FullName 'target\release\WebView2Loader.dll' -Force
    $ico = Get-ChildItem 'target\release\build' -Recurse -Filter veil.ico | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    Copy-Item $ico.FullName 'target\release\veil.ico' -Force

    $iscc = @("$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe") |
        Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $iscc) { throw 'Inno Setup 6 not found: winget install JRSoftware.InnoSetup' }
    & $iscc /Q 'installer\veil.iss'
    if ($LASTEXITCODE) { throw 'installer build failed' }
    Get-ChildItem dist\*.exe | Select-Object FullName, Length
} finally {
    Pop-Location
}
