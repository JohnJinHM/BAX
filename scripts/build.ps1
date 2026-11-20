#Requires -Version 7.0
param(
    [ValidateSet('Dev', 'Portable', 'Installer')][string]$Mode = 'Dev',
    [ValidateSet('Full', 'Lite')][string]$Edition = 'Full',
    [switch]$SkipOcrBuild,
    [switch]$SignUpdate,
    [switch]$SkipNpmInstall
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$previousTarget = $env:CARGO_TARGET_DIR
$stage = $null
$temporaryZip = $null
Push-Location $repo
try {
    if ($SignUpdate -and $Mode -ne 'Installer') { throw '-SignUpdate 只用于 Installer' }
    if ($SignUpdate -and -not $env:TAURI_SIGNING_PRIVATE_KEY) { throw '签名发布需要 TAURI_SIGNING_PRIVATE_KEY' }
    $Mode = @{'dev'='Dev'; 'portable'='Portable'; 'installer'='Installer'}[$Mode.ToLowerInvariant()]
    $Edition = @{'full'='Full'; 'lite'='Lite'}[$Edition.ToLowerInvariant()]
    if ($Edition -eq 'Full') {
        if (-not $SkipOcrBuild) {
            & (Join-Path $PSScriptRoot 'build-ocr-native.ps1')
            if ($LASTEXITCODE -ne 0) { throw '原生 OCR 打包或自检失败' }
        }
        $runtime = Join-Path $repo '.local/ocr-dist/bax-ocr'
        $manifest = Get-Content (Join-Path $runtime 'manifest.json') -Raw | ConvertFrom-Json
        if (-not $manifest.native) { throw '请先构建原生 OCR，不能复用旧 Python 产物' }
        $required = @('bax-ocr.exe', 'onnxruntime.dll', 'onnxruntime_providers_shared.dll',
            'msvcp140.dll', 'msvcp140_1.dll', 'vcruntime140.dll', 'vcruntime140_1.dll',
            'licenses/ONNXRuntime-LICENSE.txt', 'licenses/ONNXRuntime-ThirdPartyNotices.txt', 'licenses/PaddleOCR.txt')
        foreach ($file in $required) {
            if (-not (Test-Path -LiteralPath (Join-Path $runtime $file) -PathType Leaf)) { throw "缺少 OCR 文件：$file" }
        }
        foreach ($model in (Get-Content 'ocr/models.json' -Raw | ConvertFrom-Json)) {
            $file = Join-Path $runtime "models/$($model.file)"
            if (-not (Test-Path -LiteralPath $file) -or (Get-FileHash -LiteralPath $file).Hash -ne $model.sha256) {
                throw "OCR 模型缺失或 SHA-256 不匹配：$($model.file)"
            }
        }
    } elseif ($SkipOcrBuild) {
        throw '-SkipOcrBuild 仅适用于 Full；Lite 本身不构建 OCR'
    }
    $targetRoot = if ($previousTarget) { [IO.Path]::GetFullPath($previousTarget, $repo) } else { Join-Path $repo 'target' }
    $env:CARGO_TARGET_DIR = Join-Path $targetRoot "editions/$($Edition.ToLowerInvariant())"
    if (-not $SkipNpmInstall) {
        npm ci --no-audit --no-fund
        if ($LASTEXITCODE -ne 0) { throw '前端依赖安装失败；已有依赖且开发服务器占用文件时，可使用 -SkipNpmInstall' }
    }
    if (-not (Test-Path -LiteralPath 'node_modules/@tauri-apps/cli/tauri.js')) { throw '缺少前端依赖，请先运行 npm ci' }
    $buildArgs = @('build')
    if ($Mode -eq 'Installer') { $buildArgs += @('--bundles', 'nsis') } else { $buildArgs += '--no-bundle' }
    if ($Mode -eq 'Dev') { $buildArgs += '--debug' }
    if ($Edition -eq 'Full') { $buildArgs += @('--features', 'ocr', '--config', 'src-tauri/tauri.ocr.conf.json') }
    if ($Mode -eq 'Installer') { $buildArgs += @('--features', 'installed-updater') }
    if ($SignUpdate) { $buildArgs += @('--config', 'src-tauri/tauri.updater.conf.json') }
    $buildArgs += @('--', '--no-default-features')
    node node_modules/@tauri-apps/cli/tauri.js @buildArgs
    if ($LASTEXITCODE -ne 0) { throw "$Mode 构建失败" }
    $buildProfile = if ($Mode -eq 'Dev') { 'debug' } else { 'release' }
    $output = Join-Path $env:CARGO_TARGET_DIR $buildProfile
    $exe = Join-Path $output 'bax-app.exe'
    if (-not (Test-Path -LiteralPath $exe)) { throw "未找到程序：$exe" }
    if ($Edition -eq 'Full') {
        # --no-bundle 不保证刷新已有资源，必须显式替换旧 Python/原生工作进程。
        $resolvedOutput = (Resolve-Path -LiteralPath $output).Path.TrimEnd('\')
        $destination = [IO.Path]::GetFullPath((Join-Path $resolvedOutput 'ocr-runtime'))
        if ((Split-Path -Parent $destination) -ne $resolvedOutput -or $destination -eq $runtime) {
            throw 'OCR 资源目标目录越界'
        }
        if (Test-Path -LiteralPath $destination) {
            if ((Get-Item -LiteralPath $destination).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'OCR 资源目标不能是链接' }
            Remove-Item -LiteralPath $destination -Recurse -Force
        }
        Copy-Item -LiteralPath $runtime -Destination $destination -Recurse
        foreach ($file in Get-ChildItem -LiteralPath $runtime -Recurse -File) {
            $copied = Join-Path $destination ([IO.Path]::GetRelativePath($runtime, $file.FullName))
            if ((Get-FileHash -LiteralPath $copied).Hash -ne (Get-FileHash -LiteralPath $file.FullName).Hash) {
                throw "OCR 资源复制校验失败：$copied"
            }
        }
        $hello = & (Join-Path $destination 'bax-ocr.exe') (Join-Path $destination 'models') --self-test
        if ($LASTEXITCODE -ne 0) { throw '输出目录 OCR 自检失败' }
        $protocol = $hello | ConvertFrom-Json
        if ($protocol.protocolVersion -ne 1 -or $protocol.runtime -ne 'native') { throw '输出目录 OCR 协议不匹配' }
    }
    if ($Mode -eq 'Dev') {
        Write-Output "Debug 程序：$exe"
        if ($Edition -eq 'Full') { Write-Output "配套资源：$(Join-Path $output 'ocr-runtime')" }
    } else {
        $version = (Get-Content 'src-tauri/tauri.conf.json' -Raw | ConvertFrom-Json).version
        if ($Mode -eq 'Installer') {
            $bundle = Join-Path $output 'bundle/nsis'
            $installer = Join-Path $bundle "BAX_$($version)_x64-setup.exe"
            $namedInstaller = Join-Path $bundle "BAX-$($Edition)_$($version)_x64-setup.exe"
            if (-not (Test-Path -LiteralPath $installer)) { throw "未找到安装包：$installer" }
            Move-Item -LiteralPath $installer -Destination $namedInstaller -Force
            if ($SignUpdate) {
                if (-not (Test-Path -LiteralPath "$installer.sig")) { throw '未生成安装包签名' }
                Move-Item -LiteralPath "$installer.sig" -Destination "$namedInstaller.sig" -Force
            }
            Write-Output "安装包：$namedInstaller"
        } else {
            $bundle = Join-Path $output 'bundle/portable'
            New-Item -ItemType Directory -Force $bundle | Out-Null
            $stage = Join-Path $bundle ([guid]::NewGuid().ToString('N'))
            $name = "BAX-$($Edition)_$($version)_x64-portable"
            $package = Join-Path $stage $name
            New-Item -ItemType Directory -Force $package | Out-Null
            Copy-Item -LiteralPath $exe -Destination $package
            if ($Edition -eq 'Full') {
                Copy-Item -LiteralPath (Join-Path $output 'ocr-runtime') -Destination $package -Recurse
            }
            Copy-Item -LiteralPath (Join-Path $repo 'LICENSE'), (Join-Path $repo 'NOTICE.md') -Destination $package
            $launcher = @('@echo off', 'setlocal', 'set "BAX_DATA_DIR=%~dp0data"', 'start "" "%~dp0bax-app.exe" %*')
            [IO.File]::WriteAllText((Join-Path $package 'start.cmd'), ($launcher -join "`r`n") + "`r`n", [Text.Encoding]::ASCII)
            @"
BAX $Edition 便携版

解压到可写目录，通过 start.cmd 启动。数据库、设置和诊断日志保存在旁边的 data 目录。
直接运行 bax-app.exe 则使用 BAX_DATA_DIR 或默认的 %LOCALAPPDATA%\BAX。
需要已安装 WebView2 Runtime；此 ZIP 不自动安装它，WebView2 自身缓存也不保证位于 data 内。
切换版本前退出已运行的 BAX。更新时保留旧 data 文件夹，复制到新版本启动器旁边。
Full 必须保留完整的 ocr-runtime 文件夹。ZIP 不包含已有用户数据。
"@ | Set-Content -LiteralPath (Join-Path $package 'README.txt') -Encoding utf8
            $temporaryZip = Join-Path $bundle "$([guid]::NewGuid().ToString('N')).zip"
            [IO.Compression.ZipFile]::CreateFromDirectory($stage, $temporaryZip)
            $zip = Join-Path $bundle "$name.zip"
            Move-Item -LiteralPath $temporaryZip -Destination $zip -Force
            Write-Output "便携 ZIP：$zip"
        }
    }
} finally {
    try {
        # 只清理本次创建的暂存目录，不触碰用户解压后的 data。
        if ($stage -and (Test-Path -LiteralPath $stage)) {
            $resolvedStage = (Resolve-Path -LiteralPath $stage).Path
            $resolvedBundle = (Resolve-Path -LiteralPath $bundle).Path.TrimEnd('\') + '\'
            if (-not $resolvedStage.StartsWith($resolvedBundle, [StringComparison]::OrdinalIgnoreCase)) { throw '暂存目录越界' }
            Remove-Item -LiteralPath $resolvedStage -Recurse -Force
        }
        if ($temporaryZip -and (Test-Path -LiteralPath $temporaryZip)) { Remove-Item -LiteralPath $temporaryZip -Force }
    } finally {
        $env:CARGO_TARGET_DIR = $previousTarget
        Pop-Location
    }
}
