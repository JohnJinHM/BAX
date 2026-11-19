#Requires -Version 7.0
# 兼容旧入口，构建逻辑统一放在 build.ps1。
param(
    [ValidateSet('Full', 'Lite')][string]$Edition = 'Full',
    [switch]$SkipOcrBuild,
    [switch]$SkipNpmInstall
)
& (Join-Path $PSScriptRoot 'build.ps1') -Mode Installer -Edition $Edition -SkipOcrBuild:$SkipOcrBuild -SkipNpmInstall:$SkipNpmInstall
