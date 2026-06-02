# install.ps1 — install the aipdf CLI on Windows
#
# Usage (remote, from PowerShell):
#   irm https://aipdf.lackofabetter.work/install.ps1 | iex
#
# Override repository:
#   $env:AIPDF_REPO = "https://github.com/your-fork/ai.pdf"; irm ... | iex

$ErrorActionPreference = "Stop"

$repo     = if ($env:AIPDF_REPO) { $env:AIPDF_REPO } else { "https://github.com/tanmaysheoran/ai.pdf" }
$artifact = "aipdf-x86_64-windows.zip"
$url      = "$repo/releases/latest/download/$artifact"

# Install into ~/.cargo/bin (consistent with the Unix script) or fall back to %LOCALAPPDATA%\aipdf\bin
$cargoHome  = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { "$env:USERPROFILE\.cargo" }
$installDir = "$cargoHome\bin"

if (-not (Test-Path $installDir)) {
    New-Item -ItemType Directory -Path $installDir -Force | Out-Null
}

$tmp = Join-Path $env:TEMP "aipdf-install-$PID"
New-Item -ItemType Directory -Path $tmp -Force | Out-Null

try {
    Write-Host "Downloading $artifact..."
    $zipPath = Join-Path $tmp $artifact
    Invoke-WebRequest -Uri $url -OutFile $zipPath -UseBasicParsing

    Write-Host "Extracting..."
    Expand-Archive -Path $zipPath -DestinationPath $tmp -Force

    $exePath = Join-Path $tmp "aipdf.exe"
    if (-not (Test-Path $exePath)) {
        throw "aipdf.exe not found in archive — please file an issue at $repo/issues"
    }

    Copy-Item $exePath -Destination $installDir -Force
    Write-Host ""
    Write-Host "✓ aipdf installed → $installDir\aipdf.exe" -ForegroundColor Green
    Write-Host ""
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

# Add install dir to user PATH if not already present
$userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($userPath -notlike "*$installDir*") {
    [Environment]::SetEnvironmentVariable("PATH", "$installDir;$userPath", "User")
    Write-Host "Added $installDir to your user PATH." -ForegroundColor Yellow
    Write-Host "Restart your terminal (or run: `$env:PATH += `";$installDir`") to use aipdf."
    Write-Host ""
}

Write-Host "Get started:"
Write-Host "  aipdf --help"
Write-Host "  aipdf build samples\minimal.xml -o out.ai.pdf"
Write-Host "  aipdf inspect out.ai.pdf"
