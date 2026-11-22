$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    cargo build --release --workspace
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    $version = (Get-Item "target\release\fnoob-app.exe").VersionInfo.ProductVersion
    $name = "feeling-noob-$version-windows-x64"
    $stage = Join-Path "dist" $name
    if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
    New-Item -ItemType Directory -Force -Path $stage | Out-Null
    Copy-Item "target\release\fnoob-app.exe", "target\release\fnoob-scan.exe", "LICENSE" $stage
    Copy-Item "installer\install.ps1", "installer\uninstall.ps1", "installer\Install.cmd", "installer\Uninstall.cmd" $stage
    $zip = Join-Path "dist" "$name.zip"
    if (Test-Path $zip) { Remove-Item -Force $zip }
    Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip
    Write-Host "Package: $zip"
}
finally {
    Pop-Location
}
