#Requires -Version 5.1
[CmdletBinding()]
param(
    [switch]$Installer,
    [string]$SevenZip,
    [string]$InnoSetup
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:OS -ne 'Windows_NT') { throw 'Run this script on Windows.' }
$repo = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$dist = Join-Path $repo 'dist/windows'
$cache = Join-Path $repo 'target/packaging/downloads'
New-Item -ItemType Directory -Force -Path $dist, $cache | Out-Null
$manifest = Get-Content (Join-Path $PSScriptRoot 'dependencies.json') -Raw | ConvertFrom-Json
$metadata = & cargo metadata --no-deps --format-version 1 --manifest-path (Join-Path $repo 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed. Install Rust with the MSVC toolchain.' }
$metadata = $metadata | ConvertFrom-Json
$version = ($metadata.packages | Where-Object name -eq 'media-launcher').version
$bundle = Join-Path $dist "media-launcher-$version-windows-x64"
$curl = (Get-Command curl.exe -ErrorAction Stop).Source
if (-not $SevenZip) {
    $command = Get-Command 7z.exe -ErrorAction SilentlyContinue
    if ($command) { $SevenZip = $command.Source }
    elseif (Test-Path "$env:ProgramFiles/7-Zip/7z.exe") { $SevenZip = "$env:ProgramFiles/7-Zip/7z.exe" }
    else { throw 'Install 7-Zip or pass -SevenZip C:\path\to\7z.exe.' }
}
# Static MSVC CRT for our executable; third-party binaries have their own runtimes.
$oldFlags = $env:RUSTFLAGS
try {
    $env:RUSTFLAGS = "$oldFlags -C target-feature=+crt-static"
    & cargo build --locked --release --target x86_64-pc-windows-msvc --manifest-path (Join-Path $repo 'Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed. Install the x64 MSVC Rust target and Visual Studio C++ Build Tools.' }
} finally { $env:RUSTFLAGS = $oldFlags }
# Verify all downloads before creating or replacing the output bundle.
foreach ($asset in $manifest.assets) {
    $download = Join-Path $cache $asset.file
    $cachedHash = $null
    if (Test-Path $download) {
        $cachedHash = (Get-FileHash $download -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    if ($cachedHash -eq $asset.sha256) { continue }
    if ($cachedHash) {
        Write-Warning "Replacing invalid cached $($asset.file): expected $($asset.sha256), got $cachedHash"
    }
    Write-Host "Downloading $($asset.name) $($asset.version)"
    $partial = "$download.partial"
    try {
        # Use curl.exe explicitly: Windows PowerShell aliases 'curl' to
        # Invoke-WebRequest, which produced a different archive in native testing.
        & $curl --fail --location --retry 3 --retry-delay 1 --output $partial $asset.url
        if ($LASTEXITCODE -ne 0) { throw "Download failed: $($asset.url)" }
        $actualHash = (Get-FileHash $partial -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actualHash -ne $asset.sha256) {
            $bytes = (Get-Item $partial).Length
            throw "Checksum mismatch for $($asset.file) ($bytes bytes). Expected $($asset.sha256), got $actualHash. URL: $($asset.url)"
        }
        Move-Item -LiteralPath $partial -Destination $download -Force
    } finally {
        if (Test-Path $partial) { Remove-Item -LiteralPath $partial -Force }
    }
}
# Preserve earlier/partial bundles while allowing a failed build to be retried.
if (Test-Path $bundle) {
    $previous = "$bundle.previous-$([Guid]::NewGuid().ToString('N'))"
    Move-Item -LiteralPath $bundle -Destination $previous
    Write-Host "Previous output preserved at: $previous"
}
New-Item -ItemType Directory -Path $bundle | Out-Null
Copy-Item (Join-Path $metadata.target_directory 'x86_64-pc-windows-msvc/release/media-launcher.exe') $bundle
foreach ($asset in $manifest.assets) {
    $download = Join-Path $cache $asset.file
    switch ($asset.name) {
        'libmpv' {
            $unpack = Join-Path $bundle '_mpv'
            & $SevenZip x '-y' "-o$unpack" $download | Out-Host
            if ($LASTEXITCODE -ne 0) { throw 'libmpv extraction failed.' }
            Get-ChildItem $unpack -Recurse -Filter '*.dll' | Copy-Item -Destination $bundle
            Remove-Item $unpack -Recurse -Force
            if (-not (Test-Path (Join-Path $bundle 'libmpv-2.dll'))) { throw 'Archive did not contain libmpv-2.dll.' }
        }
        'yt-dlp' { Copy-Item $download (Join-Path $bundle 'yt-dlp.exe') }
        'deno' { Expand-Archive -LiteralPath $download -DestinationPath $bundle }
        'vulkan-loader' {
            $unpack = Join-Path $bundle '_vulkan'
            Expand-Archive -LiteralPath $download -DestinationPath $unpack
            $components = Join-Path $unpack "VulkanRT-X64-$($asset.version)-Components"
            Copy-Item (Join-Path $components 'x64/vulkan-1.dll') $bundle
            Copy-Item (Join-Path $components 'VulkanRT-License.txt') $bundle
            Remove-Item $unpack -Recurse -Force
        }
    }
}
Copy-Item (Join-Path $repo 'LICENSE') (Join-Path $bundle 'LICENSE.txt')
Copy-Item (Join-Path $PSScriptRoot 'licenses') $bundle -Recurse
Copy-Item (Join-Path $PSScriptRoot 'THIRD-PARTY.md') $bundle
Copy-Item (Join-Path $PSScriptRoot 'dependencies.json') $bundle
Copy-Item (Join-Path $repo 'packaging/config.toml') (Join-Path $bundle 'config.example.toml')
Copy-Item (Join-Path $PSScriptRoot 'BUNDLE-README.txt') (Join-Path $bundle 'README.txt')
# No updater yet. This namespace is reserved without creating or executing files.
& (Join-Path $bundle 'yt-dlp.exe') --version
if ($LASTEXITCODE -ne 0) { throw 'Bundled yt-dlp cannot run.' }
& (Join-Path $bundle 'deno.exe') --version
if ($LASTEXITCODE -ne 0) { throw 'Bundled Deno cannot run.' }
$zip = "$bundle.zip"
Compress-Archive -Path $bundle -DestinationPath $zip -Force
Write-Host "Bundle: $bundle"
Write-Host "Portable ZIP: $zip"
if ($Installer) {
    if (-not $InnoSetup) {
        $command = Get-Command ISCC.exe -ErrorAction SilentlyContinue
        if ($command) { $InnoSetup = $command.Source }
        else {
            $InnoSetup = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6/ISCC.exe'
        }
    }
    if (-not (Test-Path $InnoSetup)) { throw 'Install Inno Setup 6 or pass -InnoSetup C:\path\to\ISCC.exe.' }
    & $InnoSetup "/DAppVersion=$version" "/DBundleDir=$bundle" "/DOutputDir=$dist" (Join-Path $PSScriptRoot 'installer.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed.' }
}
