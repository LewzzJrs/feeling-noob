param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\feeling-noob"),
    [string]$ShortcutDir = (Join-Path ([Environment]::GetFolderPath("Programs")) "feeling-noob"),
    [switch]$SkipRegistry
)
$ErrorActionPreference = "Stop"
$source = $PSScriptRoot
$version = (Get-Item (Join-Path $source "fnoob-app.exe")).VersionInfo.ProductVersion

if (Get-Process fnoob-app -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$InstallDir*" }) {
    Write-Host "Close feeling-noob first, then run the installer again."
    exit 1
}

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
foreach ($file in "fnoob-app.exe", "fnoob-scan.exe", "LICENSE", "uninstall.ps1", "Uninstall.cmd") {
    Copy-Item -Force (Join-Path $source $file) (Join-Path $InstallDir $file)
}

New-Item -ItemType Directory -Force -Path $ShortcutDir | Out-Null
$shell = New-Object -ComObject WScript.Shell
$link = $shell.CreateShortcut((Join-Path $ShortcutDir "feeling-noob.lnk"))
$link.TargetPath = Join-Path $InstallDir "fnoob-app.exe"
$link.WorkingDirectory = $InstallDir
$link.IconLocation = (Join-Path $InstallDir "fnoob-app.exe") + ",0"
$link.Save()

if (-not $SkipRegistry) {
    $key = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\feeling-noob"
    New-Item -Force -Path $key | Out-Null
    $values = @{
        DisplayName     = "feeling-noob"
        DisplayVersion  = $version
        Publisher       = "LewzzJrs"
        InstallLocation = $InstallDir
        DisplayIcon     = (Join-Path $InstallDir "fnoob-app.exe")
        UninstallString = "`"" + (Join-Path $InstallDir "Uninstall.cmd") + "`""
    }
    foreach ($name in $values.Keys) {
        New-ItemProperty -Force -Path $key -Name $name -Value $values[$name] | Out-Null
    }
    New-ItemProperty -Force -Path $key -Name NoModify -Value 1 -PropertyType DWord | Out-Null
    New-ItemProperty -Force -Path $key -Name NoRepair -Value 1 -PropertyType DWord | Out-Null
}

Write-Host "feeling-noob $version is installed in $InstallDir"
Write-Host "Start it from the Start menu."
