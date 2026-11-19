param([switch]$SkipOcrBuild)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location $repo
try {
    if (-not $SkipOcrBuild) {
        $python = Join-Path $repo '.local/ocr-build-env/Scripts/python.exe'
        if (-not (Test-Path -LiteralPath $python)) {
            python -m venv (Join-Path $repo '.local/ocr-build-env')
            if ($LASTEXITCODE -ne 0) { throw '无法创建 OCR 构建环境' }
        }
        & $python -m pip install --disable-pip-version-check -r ocr/requirements-build.txt
        if ($LASTEXITCODE -ne 0) { throw 'OCR 构建依赖安装失败' }
        & $python scripts/build-ocr.py
        if ($LASTEXITCODE -ne 0) { throw 'OCR 打包或离线自检失败' }
    }
    if (-not (Test-Path -LiteralPath '.local/ocr-dist/bax-ocr/bax-ocr.exe')) { throw '缺少 OCR 构建产物' }
    npm ci --no-audit --no-fund
    if ($LASTEXITCODE -ne 0) { throw '前端依赖安装失败' }
    node node_modules/@tauri-apps/cli/tauri.js build --bundles nsis --config src-tauri/tauri.ocr.conf.json
    if ($LASTEXITCODE -ne 0) { throw '安装包构建失败' }
} finally { Pop-Location }
