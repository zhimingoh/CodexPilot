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
  throw "This script only supports Windows MSVC targets. Detected target: $TargetTriple"
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

Write-Host "Building launcher sidecar for $TargetTriple ($Profile)"
cargo build -p codex-pilot-launcher @CargoProfileFlag --target $TargetTriple
Copy-Item (Join-Path $RootDir "target\$TargetTriple\$TargetProfileDir\codex-pilot-launcher.exe") $SidecarPath -Force

Push-Location $ManagerDir
try {
  $npmCmd = Get-Command npm.cmd -ErrorAction SilentlyContinue
  if (-not $npmCmd) {
    throw "npm.cmd not found in PATH. Install Node.js or add npm.cmd to PATH before packaging."
  }

  $tauriArgs = @("run", "tauri", "--", "build", "--target", $TargetTriple)
  if ($Profile -ne "release") {
    $tauriArgs += @("--debug")
  }

  Write-Host "Building CodexPilot manager with Tauri"
  & $npmCmd.Source @tauriArgs

  $bundleRoot = Join-Path $RootDir "target\$TargetTriple\$TargetProfileDir\bundle"
  $nsisInstaller = Get-ChildItem -Path (Join-Path $bundleRoot "nsis") -Filter "CodexPilot_$($Version)_*_setup.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
  if (-not $nsisInstaller) {
    $nsisInstaller = Get-ChildItem -Path (Join-Path $bundleRoot "nsis") -Filter "CodexPilot_$($Version)_*.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
  }
  $msiInstaller = Get-ChildItem -Path (Join-Path $bundleRoot "msi") -Filter "*$Version*.msi" -ErrorAction SilentlyContinue | Select-Object -First 1

  if (-not $nsisInstaller -and -not $msiInstaller) {
    throw "Tauri build completed, but no Windows installer was found under $bundleRoot"
  }

  $distDir = Join-Path $RootDir "dist\windows"
  New-Item -ItemType Directory -Force $distDir | Out-Null

  if ($nsisInstaller) {
    $destExe = Join-Path $distDir "CodexPilot-$Version-windows-x64-setup.exe"
    Copy-Item $nsisInstaller.FullName $destExe -Force
    Write-Host "Windows installer ready: $destExe"
  }

  if ($msiInstaller) {
    $destMsi = Join-Path $distDir "CodexPilot-$Version-windows-x64.msi"
    Copy-Item $msiInstaller.FullName $destMsi -Force
    Write-Host "Windows MSI ready: $destMsi"
  }
} finally {
  Pop-Location
}
