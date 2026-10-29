[CmdletBinding()]
param([switch]$Launch)
$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'install.ps1') -WorkbenchOnly -Launch:$Launch
