param(
    [Parameter(Mandatory=$true)]
    [string]$NewVersion
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Replace-VersionJson {
    param(
        [string]$Path,
        [string]$Current,
        [string]$New
    )

    $content = Get-Content $Path -Raw
    $pattern = '"version"\s*:\s*"' + [Regex]::Escape($Current) + '"'
    if (-not [Regex]::IsMatch($content, $pattern)) {
        throw "Version field not found in $Path"
    }
    $updated = [Regex]::Replace($content, $pattern, '"version": "' + $New + '"', 1)
    Set-Content -Path $Path -Value $updated -Encoding UTF8
    Write-Host "Updated $Path"
}

function Replace-VersionToml {
    param(
        [string]$Path,
        [string]$Current,
        [string]$New
    )

    $content = Get-Content $Path -Raw
    $pattern = 'version\s*=\s*"' + [Regex]::Escape($Current) + '"'
    if (-not [Regex]::IsMatch($content, $pattern)) {
        throw "Version field not found in $Path"
    }
    $updated = [Regex]::Replace($content, $pattern, 'version = "' + $New + '"', 1)
    Set-Content -Path $Path -Value $updated -Encoding UTF8
    Write-Host "Updated $Path"
}

$root = Split-Path -Parent (Resolve-Path $MyInvocation.MyCommand.Path)
$projectRoot = Resolve-Path (Join-Path $root '..')
Write-Host "Working directory: $projectRoot"

$packagePath = Resolve-Path (Join-Path $projectRoot 'package.json')
$currentVersion = (Get-Content $packagePath -Raw | ConvertFrom-Json).version
if (-not $currentVersion) {
    throw 'Failed to determine current version from package.json.'
}

Write-Host "Current version: $currentVersion"
Write-Host "Updating to:    $NewVersion"

try {
    Replace-VersionJson -Path $packagePath -Current $currentVersion -New $NewVersion
    Replace-VersionToml -Path (Resolve-Path (Join-Path $projectRoot 'src-tauri\Cargo.toml')) -Current $currentVersion -New $NewVersion
    Replace-VersionJson -Path (Resolve-Path (Join-Path $projectRoot 'src-tauri\tauri.conf.json')) -Current $currentVersion -New $NewVersion
    Replace-VersionJson -Path (Resolve-Path (Join-Path $projectRoot 'static\version.json')) -Current $currentVersion -New $NewVersion
    Write-Host 'All files updated successfully.'
    exit 0
}
catch {
    Write-Error $_
    exit 1
}

