$ErrorActionPreference = "Stop"

$RootDir = Resolve-Path (Join-Path $PSScriptRoot "..")
$ManagerDir = Join-Path $RootDir "apps\codex-pilot-manager"
$TargetTriple = if ($env:TARGET_TRIPLE) { $env:TARGET_TRIPLE } else { "x86_64-pc-windows-msvc" }
$Profile = if ($env:PROFILE) { $env:PROFILE } else { "release" }
$Version = if ($env:VERSION) {
  $env:VERSION
} else {
  $config = Get-Content (Join-Path $ManagerDir "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json
  $config.version
}

if ($TargetTriple -notlike "*windows-msvc") {
  throw "当前脚本只用于 Windows MSVC 打包，检测到 target：$TargetTriple"
}

$CargoProfileFlag = @()
$TargetProfileDir = "debug"
if ($Profile -eq "release") {
  $CargoProfileFlag = @("--release")
  $TargetProfileDir = "release"
}

$SidecarDir = Join-Path $ManagerDir "src-tauri\binaries"
$SidecarPath = Join-Path $SidecarDir "codex-pilot-launcher-$TargetTriple.exe"
New-Item -ItemType Directory -Force $SidecarDir | Out-Null

Write-Host "构建 launcher sidecar：$TargetTriple ($Profile)"
cargo build -p codex-pilot-launcher @CargoProfileFlag --target $TargetTriple
Copy-Item (Join-Path $RootDir "target\$TargetTriple\$TargetProfileDir\codex-pilot-launcher.exe") $SidecarPath -Force

Write-Host "构建 Manager 前端"
Push-Location $ManagerDir
try {
  npm run vite:build
} finally {
  Pop-Location
}

Write-Host "构建 Manager 可执行文件"
cargo build -p codex-pilot-manager @CargoProfileFlag --target $TargetTriple

$AppStageDir = Join-Path $RootDir "dist\windows\app"
New-Item -ItemType Directory -Force $AppStageDir | Out-Null
Copy-Item (Join-Path $RootDir "target\$TargetTriple\$TargetProfileDir\codex-pilot-launcher.exe") (Join-Path $AppStageDir "codex-pilot-launcher.exe") -Force
Copy-Item (Join-Path $RootDir "target\$TargetTriple\$TargetProfileDir\codex-pilot-manager.exe") (Join-Path $AppStageDir "codex-pilot-manager.exe") -Force

$MakeNsis = Join-Path ${env:ProgramFiles(x86)} "NSIS\makensis.exe"
if (-not (Test-Path $MakeNsis)) {
  $MakeNsis = "makensis"
}

Write-Host "生成 Windows 安装包"
Push-Location (Join-Path $RootDir "scripts\installer\windows")
try {
  & $MakeNsis "/INPUTCHARSET" "UTF8" "/DVERSION=$Version" "CodexPilot.nsi"
} finally {
  Pop-Location
}

Write-Host "Windows 打包完成：$(Join-Path $RootDir "dist\windows\CodexPilot-$Version-windows-x64-setup.exe")"
