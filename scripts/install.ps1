# simse installer for Windows
# Usage: irm https://cdn.simse.dev/install.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "simsedev/simse-cli"
$BinaryName = "simse.exe"
$InstallDir = "$env:LOCALAPPDATA\simse\bin"

# ---------------------------------------------------------------------------
# Detect architecture
# ---------------------------------------------------------------------------

$Arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
switch ($Arch) {
    "X64"   { $Platform = "windows-x86_64" }
    "Arm64" { $Platform = "windows-aarch64" }

    default { Write-Error "Unsupported architecture: $Arch"; exit 1 }
}

# ---------------------------------------------------------------------------
# Get latest version
# ---------------------------------------------------------------------------

Write-Host "Fetching latest version..." -ForegroundColor Cyan
$Release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
$Version = $Release.tag_name

if (-not $Version) {
    Write-Error "Could not determine latest version"
    exit 1
}

# ---------------------------------------------------------------------------
# Download
# ---------------------------------------------------------------------------

$FileName = "simse-${Platform}.zip"
$Url = "https://github.com/$Repo/releases/download/$Version/$FileName"

Write-Host "Downloading simse $Version for $Platform..." -ForegroundColor Cyan

$TmpDir = New-Item -ItemType Directory -Path (Join-Path $env:TEMP "simse-install-$(Get-Random)")
$TmpFile = Join-Path $TmpDir $FileName

try {
    Invoke-WebRequest -Uri $Url -OutFile $TmpFile -UseBasicParsing
} catch {
    Write-Error "Download failed: $_"
    exit 1
}

# ---------------------------------------------------------------------------
# Extract and install
# ---------------------------------------------------------------------------

Expand-Archive -Path $TmpFile -DestinationPath $TmpDir -Force

# ---------------------------------------------------------------------------
# Remove old versions from common locations
# ---------------------------------------------------------------------------

$OldLocations = @(
    "$env:ProgramFiles\simse\simse.exe",
    "$env:USERPROFILE\.cargo\bin\simse.exe",
    "$env:USERPROFILE\bin\simse.exe",
    "$env:USERPROFILE\scoop\shims\simse.exe"
)

foreach ($OldPath in $OldLocations) {
    if (Test-Path $OldPath) {
        Write-Host "Removing old simse at $OldPath" -ForegroundColor Yellow
        Remove-Item -Force $OldPath -ErrorAction SilentlyContinue
    }
}

# Also remove from install dir if it exists (will be replaced)
$ExistingInstall = Join-Path $InstallDir $BinaryName
if (Test-Path $ExistingInstall) {
    Remove-Item -Force $ExistingInstall -ErrorAction SilentlyContinue
}

if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

$SrcBin = Join-Path $TmpDir "bin\$BinaryName"
$SrcPlugins = Join-Path $TmpDir "share\simse\plugins"
if (-not (Test-Path $SrcBin)) {
    Write-Error "Archive missing bin\$BinaryName"
    exit 1
}

Copy-Item -Path $SrcBin -Destination (Join-Path $InstallDir $BinaryName) -Force

# Plugins -> <prefix>\share\simse\plugins, where <prefix> is the parent of
# the bin dir, so the binary's <exe>\..\share\simse\plugins lookup finds them.
if (Test-Path $SrcPlugins) {
    $Prefix = Split-Path -Parent $InstallDir
    $PluginDest = Join-Path $Prefix "share\simse\plugins"
    if (Test-Path $PluginDest) {
        Remove-Item -Recurse -Force $PluginDest
    }
    New-Item -ItemType Directory -Path (Split-Path -Parent $PluginDest) -Force | Out-Null
    Copy-Item -Path $SrcPlugins -Destination $PluginDest -Recurse -Force
    Write-Host "Plugins installed to $PluginDest" -ForegroundColor Green
}

# ---------------------------------------------------------------------------
# Add to PATH if needed
# ---------------------------------------------------------------------------

$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    Write-Host "Adding $InstallDir to your PATH..." -ForegroundColor Yellow
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
    $env:Path = "$env:Path;$InstallDir"
}

# ---------------------------------------------------------------------------
# Cleanup
# ---------------------------------------------------------------------------

Remove-Item -Recurse -Force $TmpDir -ErrorAction SilentlyContinue

# ---------------------------------------------------------------------------
# Done
# ---------------------------------------------------------------------------

# ---------------------------------------------------------------------------
# Reload PATH in current session
# ---------------------------------------------------------------------------

$env:Path = [Environment]::GetEnvironmentVariable("Path", "Machine") + ";" + [Environment]::GetEnvironmentVariable("Path", "User")

Write-Host ""
Write-Host "simse $Version installed to $InstallDir\simse.exe" -ForegroundColor Green
Write-Host "Run 'simse' to get started." -ForegroundColor Green
