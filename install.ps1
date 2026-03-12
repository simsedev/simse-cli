# simse installer for Windows
# Usage: irm https://raw.githubusercontent.com/simsedev/simse-cli/main/install.ps1 | iex

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

if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

Copy-Item -Path (Join-Path $TmpDir $BinaryName) -Destination (Join-Path $InstallDir $BinaryName) -Force

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

Write-Host ""
Write-Host "simse $Version installed to $InstallDir\simse.exe" -ForegroundColor Green
Write-Host "Run 'simse' to get started." -ForegroundColor Green
Write-Host ""
Write-Host "Note: You may need to restart your terminal for PATH changes to take effect." -ForegroundColor Yellow
