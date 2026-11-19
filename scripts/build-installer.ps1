param(
    [ValidateSet('Full', 'Lite')][string]$Edition = 'Full',
    [switch]$SkipOcrBuild
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$previousTarget = $env:CARGO_TARGET_DIR
Push-Location $repo
try {
    if ($Edition -eq 'Full') {
        if (-not $SkipOcrBuild) {
            & (Join-Path $PSScriptRoot 'build-ocr-native.ps1')
            if ($LASTEXITCODE -ne 0) { throw '原生 OCR 打包或自检失败' }
        }
        if (-not (Test-Path -LiteralPath '.local/ocr-dist/bax-ocr/bax-ocr.exe')) { throw '缺少 OCR 构建产物' }
        $manifest = Get-Content '.local/ocr-dist/bax-ocr/manifest.json' -Raw | ConvertFrom-Json
        if (-not $manifest.native) { throw '请先构建原生 OCR，不能复用旧 Python 产物' }
    } elseif ($SkipOcrBuild) {
        throw '-SkipOcrBuild 仅适用于 Full；Lite 本身不构建 OCR'
    }
    # 分开 target，避免先构建 Full 后把残留资源带进 Lite。
    $targetRoot = if ($previousTarget) { [System.IO.Path]::GetFullPath($previousTarget, $repo) } else { Join-Path $repo 'target' }
    $env:CARGO_TARGET_DIR = Join-Path $targetRoot "editions/$($Edition.ToLowerInvariant())"
    npm ci --no-audit --no-fund
    if ($LASTEXITCODE -ne 0) { throw '前端依赖安装失败' }
    $buildArgs = @('build', '--bundles', 'nsis')
    if ($Edition -eq 'Full') { $buildArgs += @('--features', 'ocr', '--config', 'src-tauri/tauri.ocr.conf.json') }
    $buildArgs += @('--', '--no-default-features')
    node node_modules/@tauri-apps/cli/tauri.js @buildArgs
    if ($LASTEXITCODE -ne 0) { throw '安装包构建失败' }
    $version = (Get-Content 'src-tauri/tauri.conf.json' -Raw | ConvertFrom-Json).version
    $bundle = Join-Path $env:CARGO_TARGET_DIR 'release/bundle/nsis'
    $installer = Join-Path $bundle "BAX_$($version)_x64-setup.exe"
    $namedInstaller = Join-Path $bundle "BAX-$($Edition)_$($version)_x64-setup.exe"
    if (-not (Test-Path -LiteralPath $installer)) { throw "未找到安装包：$installer" }
    Move-Item -LiteralPath $installer -Destination $namedInstaller -Force
    Write-Output "安装包：$namedInstaller"
} finally {
    $env:CARGO_TARGET_DIR = $previousTarget
    Pop-Location
}
