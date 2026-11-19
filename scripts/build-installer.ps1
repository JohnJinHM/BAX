param([switch]$SkipOcrBuild)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location $repo
try {
    if (-not $SkipOcrBuild) {
        & (Join-Path $PSScriptRoot 'build-ocr-native.ps1')
        if ($LASTEXITCODE -ne 0) { throw '原生 OCR 打包或自检失败' }
    }
    if (-not (Test-Path -LiteralPath '.local/ocr-dist/bax-ocr/bax-ocr.exe')) { throw '缺少 OCR 构建产物' }
    $manifest = Get-Content '.local/ocr-dist/bax-ocr/manifest.json' -Raw | ConvertFrom-Json
    if (-not $manifest.native) { throw '请先构建原生 OCR，不能复用旧 Python 产物' }
    npm ci --no-audit --no-fund
    if ($LASTEXITCODE -ne 0) { throw '前端依赖安装失败' }
    node node_modules/@tauri-apps/cli/tauri.js build --bundles nsis --config src-tauri/tauri.ocr.conf.json
    if ($LASTEXITCODE -ne 0) { throw '安装包构建失败' }
} finally { Pop-Location }
