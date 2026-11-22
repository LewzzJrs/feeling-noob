param(
    [string]$InstallDir = $PSScriptRoot,
    [string]$ShortcutDir = (Join-Path ([Environment]::GetFolderPath("Programs")) "feeling-noob"),
    [switch]$SkipRegistry
)
$ErrorActionPreference = "Stop"

if (Get-Process fnoob-app -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$InstallDir*" }) {
    Write-Host "Close feeling-noob first, then run the uninstaller again."
    exit 1
}

if (Test-Path $ShortcutDir) {
    Remove-Item -Recurse -Force $ShortcutDir
}
if (-not $SkipRegistry) {
    Remove-Item -Force -ErrorAction SilentlyContinue "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\feeling-noob"
}
foreach ($file in "fnoob-app.exe", "fnoob-scan.exe", "LICENSE", "Uninstall.cmd", "uninstall.ps1") {
    Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $InstallDir $file)
}
if (-not (Get-ChildItem $InstallDir -ErrorAction SilentlyContinue)) {
    Remove-Item -Force -ErrorAction SilentlyContinue $InstallDir
}
Write-Host "feeling-noob is removed. Your projects and settings in %APPDATA%\feeling-noob are kept."
