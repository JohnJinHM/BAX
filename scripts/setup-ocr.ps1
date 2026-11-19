param([string]$DataDir = $(if ($env:BAX_DATA_DIR) { $env:BAX_DATA_DIR } else { Join-Path $env:LOCALAPPDATA 'BAX' }))
$ErrorActionPreference = 'Stop'
$runtime = Join-Path $DataDir 'ocr'
$python = Join-Path $runtime 'Scripts/python.exe'
if (-not (Test-Path -LiteralPath $python)) {
    python -m venv $runtime
    if ($LASTEXITCODE -ne 0) { throw '创建 OCR 环境失败；需要 Python 3.10 或更新版本。' }
}
& $python -m pip install --disable-pip-version-check 'rapidocr==3.10.0' 'onnxruntime==1.32.0' 'Pillow==13.0.0'
if ($LASTEXITCODE -ne 0) { throw '安装 OCR 依赖失败。' }
$models = Join-Path $runtime 'models'
& $python -c 'import sys,numpy as np; from rapidocr import RapidOCR; e=RapidOCR(params={"Global.model_root_dir":sys.argv[1],"EngineConfig.onnxruntime.intra_op_num_threads":2}); e(np.zeros((96,320,3),dtype=np.uint8),use_cls=False); e._load_rec_model(); print("OCR ready")' $models
if ($LASTEXITCODE -ne 0) { throw '下载或校验 OCR 模型失败。' }
& $python -c 'import sys,numpy as np; from rapidocr import RapidOCR,LangRec,ModelType,OCRVersion; e=RapidOCR(params={"Global.model_root_dir":sys.argv[1],"Rec.lang_type":LangRec.ESLAV,"Rec.model_type":ModelType.MOBILE,"Rec.ocr_version":OCRVersion.PPOCRV5,"EngineConfig.onnxruntime.intra_op_num_threads":2}); e(np.zeros((48,160,3),dtype=np.uint8),use_det=False,use_cls=False)' $models
if ($LASTEXITCODE -ne 0) { throw '俄文 OCR 模型准备失败。' }
Write-Output "OCR 已安装：$runtime"
