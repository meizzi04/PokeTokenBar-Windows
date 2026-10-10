# Installs (or updates) the latest PokeTokenBar for Windows and starts it.
# Per-user install: no admin rights needed.
#   irm https://raw.githubusercontent.com/tkdals2317/PokeTokenBar-Windows/main/install.ps1 | iex

$ErrorActionPreference = 'Stop'
$repo = 'tkdals2317/PokeTokenBar-Windows'

Write-Host 'Finding the latest release...'
$release = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest"
$asset = $release.assets | Where-Object { $_.name -like '*-setup.exe' } | Select-Object -First 1
if (-not $asset) { throw "No installer found in release $($release.tag_name)." }

$installer = Join-Path $env:TEMP $asset.name
Write-Host "Downloading $($asset.name)..."
Invoke-WebRequest $asset.browser_download_url -OutFile $installer -UseBasicParsing
Unblock-File $installer

# Close a running copy so the installer can replace it.
Get-Process -Name 'poketokenbar' -ErrorAction SilentlyContinue | Stop-Process -Force

Write-Host 'Installing...'
Start-Process $installer -ArgumentList '/S' -Wait
Remove-Item $installer -ErrorAction SilentlyContinue

$exe = Get-ChildItem (Join-Path $env:LOCALAPPDATA 'PokeTokenBar') -Filter '*.exe' -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -notlike 'uninstall*' } | Select-Object -First 1
if (-not $exe) { throw 'Installed, but the app was not found under %LOCALAPPDATA%\PokeTokenBar.' }

Start-Process $exe.FullName
Write-Host "PokeTokenBar $($release.tag_name) installed and started."
Write-Host 'Look for the egg on your desktop and the tray icon (taskbar ^ area).'
