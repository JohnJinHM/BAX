param([switch]$SkipDownload)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location $repo
try {
    $local = Join-Path $repo '.local'
    New-Item -ItemType Directory -Force $local | Out-Null
    $sdk = Join-Path $local 'onnxruntime-sdk'
    $archive = Join-Path $local 'onnxruntime-1.22.0.nupkg'
    $sdkHash = 'd571e63a2329baacb713f441e65ad75284de354db6e1ac435fe4bebbb417986a'
    if (-not (Test-Path $archive) -or (Get-FileHash $archive).Hash -ne $sdkHash) {
        if ($SkipDownload) { throw '缺少已校验的 ONNX Runtime SDK 缓存' }
        Invoke-WebRequest 'https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime/1.22.0/microsoft.ml.onnxruntime.1.22.0.nupkg' -OutFile $archive
    }
    if ((Get-FileHash $archive).Hash -ne $sdkHash) { throw 'ONNX Runtime SDK SHA-256 不匹配' }
    Expand-Archive -LiteralPath $archive -DestinationPath $sdk -Force
    $models = Get-Content ocr/models.json -Raw | ConvertFrom-Json
    $cache = Join-Path $local 'ocr-models'
    New-Item -ItemType Directory -Force $cache | Out-Null
    foreach ($model in $models) {
        $file = Join-Path $cache $model.file
        if (-not (Test-Path $file) -or (Get-FileHash $file).Hash -ne $model.sha256) {
            if ($SkipDownload) { throw "缺少已校验的模型：$($model.file)" }
            Invoke-WebRequest $model.url -OutFile $file
        }
        if ((Get-FileHash $file).Hash -ne $model.sha256) { throw "模型 SHA-256 不匹配：$($model.file)" }
    }
    cmake -S ocr/native -B .local/native-build -A x64 "-DORT_ROOT=$sdk"
    if ($LASTEXITCODE -ne 0) { throw 'CMake 配置失败' }
    cmake --build .local/native-build --config Release
    if ($LASTEXITCODE -ne 0) { throw '原生 OCR 编译失败' }
    $dist = [IO.Path]::GetFullPath((Join-Path $local 'ocr-dist/bax-ocr'))
    if (-not $dist.StartsWith([IO.Path]::GetFullPath($local) + [IO.Path]::DirectorySeparatorChar)) { throw '产物目录越界' }
    if (Test-Path -LiteralPath $dist) {
        if ((Get-Item -LiteralPath $dist).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw '产物目录不能是链接' }
        Remove-Item -LiteralPath $dist -Recurse -Force
    }
    New-Item -ItemType Directory -Force "$dist/models", "$dist/licenses" | Out-Null
    Copy-Item .local/native-build/Release/bax-ocr.exe $dist
    Copy-Item "$sdk/runtimes/win-x64/native/*.dll" $dist
    foreach ($model in $models) { Copy-Item (Join-Path $cache $model.file) "$dist/models" }
    Copy-Item "$sdk/LICENSE" "$dist/licenses/ONNXRuntime-LICENSE.txt"
    Copy-Item "$sdk/ThirdPartyNotices.txt" "$dist/licenses/ONNXRuntime-ThirdPartyNotices.txt"
    Copy-Item ocr/licenses/PaddleOCR.txt "$dist/licenses"
    # App-local MSVC runtime for ONNX Runtime, independent of the user's Python/VC setup.
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    $vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $vs) { throw '缺少 Visual Studio C++ 构建工具' }
    $redist = Get-ChildItem "$vs/VC/Redist/MSVC" -Directory | Where-Object { $_.Name -match '^\d+\.' } | Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
    $crt = Join-Path $redist.FullName 'x64/Microsoft.VC143.CRT'
    foreach ($dll in @('msvcp140.dll','msvcp140_1.dll','vcruntime140.dll','vcruntime140_1.dll')) { Copy-Item (Join-Path $crt $dll) $dist }
    @{ runtime='ONNX Runtime CPU'; version='1.22.0'; sdkSha256=$sdkHash; models=$models; msvcRuntime=$redist.Name; layout='two-side-five-row-16x9'; native=$true } | ConvertTo-Json -Depth 6 | Set-Content "$dist/manifest.json"
    & "$dist/bax-ocr.exe" --unit-test
    if ($LASTEXITCODE -ne 0) { throw '原生 OCR 单元测试失败' }
    $ready = & "$dist/bax-ocr.exe" "$dist/models" --self-test
    if ($LASTEXITCODE -ne 0 -or -not ($ready | ConvertFrom-Json).ready) { throw '原生 OCR 模型自检失败' }
    $size = (Get-ChildItem $dist -Recurse -File | Measure-Object Length -Sum).Sum
    Write-Output "OCR runtime bytes: $size"
} finally { Pop-Location }
