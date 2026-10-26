[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'Programs\Mousa'),
    [switch]$Launch,
    [switch]$RunTests,
    [switch]$SkipInteractiveTests,
    [switch]$Workbench
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { throw 'This installer requires x64 Windows.' }
    $cargo = Get-Command cargo -ErrorAction SilentlyContinue
    if (!$cargo) {
        $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
        $cargo = Get-Command cargo -ErrorAction Stop
    }
    $nativeTools = Get-Command dumpbin.exe -ErrorAction SilentlyContinue
    if ($nativeTools) {
        # Also support a configured x64 Developer PowerShell environment.
        $bin = Split-Path $nativeTools.Source -Parent
    } else {
        $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
        if (!(Test-Path $vswhere)) { throw 'Install Visual Studio Build Tools with the Desktop development with C++ workload.' }
        $vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if (!$vs) { throw 'The Visual Studio C++ build tools are required.' }
        $msvc = Get-ChildItem "$vs\VC\Tools\MSVC" -Directory | Sort-Object Name -Descending | Select-Object -First 1
        $bin = Join-Path $msvc.FullName 'bin\Hostx64\x64'
    }
    $fxcPath = $env:GPUI_FXC_PATH
    if (!$fxcPath) {
        $fxc = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\fxc.exe" -ErrorAction SilentlyContinue |
            Sort-Object FullName -Descending | Select-Object -First 1
        if ($fxc) { $fxcPath = $fxc.FullName }
    }
    if (!$fxcPath -or !(Test-Path $fxcPath)) { throw 'Install the Windows SDK, including its shader compiler (fxc.exe), or set GPUI_FXC_PATH.' }
    $deps = Join-Path $root 'target\windows-deps'
    New-Item -ItemType Directory -Force $deps | Out-Null
    function Get-VerifiedFile($Url, $Path, $Sha256) {
        if (!(Test-Path $Path) -or (Get-FileHash $Path -Algorithm SHA256).Hash -ne $Sha256) {
            & curl.exe --fail --location --silent --show-error $Url --output "$Path.download"
            if ($LASTEXITCODE -ne 0) { throw "Download failed: $Url" }
            if ((Get-FileHash "$Path.download" -Algorithm SHA256).Hash -ne $Sha256) { throw "Checksum mismatch: $Url" }
            Move-Item -LiteralPath "$Path.download" -Destination $Path -Force
        }
    }
    Get-VerifiedFile 'https://github.com/shinchiro/mpv-winbuild-cmake/releases/download/20261021/mpv-dev-x86_64-20261021-git-e471496c56.7z' "$deps\mpv.7z" 'efc719eff3dc5954b4a5efc800632808bcec01cc9034940dd14e5eb7fe9ea633'
    $sevenZip = Get-Command 7z.exe -ErrorAction SilentlyContinue
    if ($sevenZip) {
        & $sevenZip.Source x '-y' "-o$deps" "$deps\mpv.7z" | Out-Null
    } elseif (Test-Path "$env:ProgramFiles\7-Zip\7z.exe") {
        & "$env:ProgramFiles\7-Zip\7z.exe" x '-y' "-o$deps" "$deps\mpv.7z" | Out-Null
    } else {
        & tar -xf "$deps\mpv.7z" -C $deps
    }
    if ($LASTEXITCODE -ne 0) { throw 'Could not extract libmpv.' }
    # MSVC needs an import library; the upstream archive supplies a MinGW library.
    $exports = & "$bin\dumpbin.exe" /exports "$deps\libmpv-2.dll"
    if ($LASTEXITCODE -ne 0) { throw 'Could not read libmpv exports.' }
    $symbols = $exports | ForEach-Object { if ($_ -match '^\s+\d+\s+[0-9A-F]+\s+[0-9A-F]+\s+(mpv_\w+)') { $Matches[1] } }
    if (!$symbols) { throw 'No libmpv API exports found.' }
    @('LIBRARY libmpv-2.dll', 'EXPORTS') + $symbols | Set-Content "$deps\mpv.def" -Encoding ascii
    & "$bin\lib.exe" /nologo "/def:$deps\mpv.def" "/out:$deps\mpv.lib" /machine:x64
    if ($LASTEXITCODE -ne 0) { throw 'Could not create mpv.lib.' }
    # libmpv imports the Vulkan loader even when Mousa only plays audio. Ship it
    # beside the app rather than relying on a GPU driver to install it globally.
    Get-VerifiedFile 'https://sdk.lunarg.com/sdk/download/1.4.363.0/windows/vulkan-runtime-components.zip' "$deps\vulkan.zip" 'a25a927aa8b9f0371048f1861cf88ac3b9bc9b1fb332c42d897c8ab32695769a'
    Expand-Archive -LiteralPath "$deps\vulkan.zip" -DestinationPath "$deps\vulkan" -Force
    $vulkan = Join-Path $deps 'vulkan\VulkanRT-X64-1.4.363.0-Components'
    Copy-Item "$vulkan\x64\vulkan-1.dll", "$vulkan\VulkanRT-License.txt" -Destination $deps -Force
    Get-VerifiedFile 'https://github.com/yt-dlp/yt-dlp/releases/download/2026.10.21/yt-dlp.exe' "$deps\yt-dlp.exe" '33121d192d706e62f684a06f80918e3f3a8ba52ac3a65213b6951c39c7d9f555'
    $previousLib = $env:LIB
    $previousFxc = $env:GPUI_FXC_PATH
    $previousPath = $env:PATH
    try {
        $env:LIB = "$deps;$env:LIB"
        $env:GPUI_FXC_PATH = $fxcPath
        & $cargo.Source build --release --locked
        if ($LASTEXITCODE -ne 0) { throw 'Mousa build failed.' }
        if ($Workbench) {
            # Keep GPUI debug assertions enabled in this development tool.
            & $cargo.Source build --locked --example workbench
            if ($LASTEXITCODE -ne 0) { throw 'Workbench build failed.' }
        }
        if ($RunTests) {
            $env:PATH = "$deps;$env:PATH"
            & $cargo.Source test --release --locked
            if ($LASTEXITCODE -ne 0) { throw 'Mousa tests failed.' }
            if (!$SkipInteractiveTests) {
                & $cargo.Source test --release --locked --test windows -- --ignored
                if ($LASTEXITCODE -ne 0) { throw 'Windows credential/playback smoke tests failed.' }
            }
        }
    } finally {
        $env:LIB = $previousLib
        $env:GPUI_FXC_PATH = $previousFxc
        $env:PATH = $previousPath
    }
    New-Item -ItemType Directory -Force $InstallDir | Out-Null
    Copy-Item 'target\release\mousa.exe', 'target\release\mousa-launcher.exe', "$deps\libmpv-2.dll", "$deps\yt-dlp.exe", "$deps\vulkan-1.dll", "$deps\VulkanRT-License.txt" -Destination $InstallDir -Force
    $installed = Join-Path $InstallDir 'mousa.exe'
    if ((Get-FileHash $installed).Hash -ne (Get-FileHash 'target\release\mousa.exe').Hash) { throw 'Installed executable does not match the build.' }
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut((Join-Path ([Environment]::GetFolderPath('Programs')) 'Mousa.lnk'))
    $launcher = Join-Path $InstallDir 'mousa-launcher.exe'
    $registration = Start-Process -FilePath $launcher -ArgumentList '--register' -Wait -PassThru
    if ($registration.ExitCode -ne 0) { throw 'Could not register the managed installation.' }
    $shortcut.TargetPath = $launcher
    $shortcut.WorkingDirectory = $InstallDir
    $shortcut.Description = 'Mousa music player'
    $shortcut.IconLocation = "$installed,0"
    $shortcut.Save()
    Write-Host "Installed Mousa to $InstallDir (Start menu: Mousa)."
    if ($Workbench) {
        $workbenchDir = Join-Path $InstallDir 'Workbench'
        New-Item -ItemType Directory -Force $workbenchDir | Out-Null
        Copy-Item 'target\debug\examples\workbench.exe', "$deps\libmpv-2.dll" -Destination $workbenchDir -Force
        $workbenchExe = Join-Path $workbenchDir 'workbench.exe'
        if ((Get-FileHash $workbenchExe).Hash -ne (Get-FileHash 'target\debug\examples\workbench.exe').Hash) {
            throw 'Installed workbench does not match the build.'
        }
        $workbenchShortcut = $shell.CreateShortcut((Join-Path ([Environment]::GetFolderPath('Programs')) 'Mousa Workbench.lnk'))
        $workbenchShortcut.TargetPath = $workbenchExe
        $workbenchShortcut.WorkingDirectory = $workbenchDir
        $workbenchShortcut.Description = 'Mousa UI stories and visual states'
        $workbenchShortcut.IconLocation = "$installed,0"
        $workbenchShortcut.Save()
        Write-Host 'Installed Mousa Workbench (Start menu: Mousa Workbench).'
    }
    if ($Launch) { Start-Process -FilePath $launcher -WorkingDirectory $InstallDir }
} finally { Pop-Location }
