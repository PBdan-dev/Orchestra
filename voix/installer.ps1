# Installe le module voix d'Orchestra dans %USERPROFILE%\Orchestra\voix (aucun droit administrateur).
#
#   powershell -ExecutionPolicy Bypass -File voix\installer.ps1 [-Etape tout|python|transcribe|modeles]
#
# Téléchargements (environ 12 Go, validés par PB le 2026-10-27) :
#   PyTorch CUDA + Pipecat + faster-qwen3-tts . ~3 Go
#   Voxtral Mini 4B Realtime Q8_0 (Apache 2.0)  4,7 Go
#   Qwen3-TTS 1.7B CustomVoice (Apache 2.0) ... 4,5 Go
# Puis compilation de transcribe.cpp (MIT) avec CUDA.

param([string]$Etape = "tout")
$ErrorActionPreference = "Stop"

$Home_ = if ($env:ORCHESTRA_HOME) { $env:ORCHESTRA_HOME } else { Join-Path $env:USERPROFILE "Orchestra" }
$Voix = Join-Path $Home_ "voix"
$Env_ = Join-Path $Voix "env"
$Modeles = Join-Path $Voix "modeles"
$Src = Split-Path -Parent $MyInvocation.MyCommand.Path
$Py = Join-Path $Env_ "Scripts\python.exe"
New-Item -ItemType Directory -Force $Voix, $Modeles, (Join-Path $Voix "references"), (Join-Path $Voix "mesures") | Out-Null

function Etape($nom) { Write-Host ""; Write-Host "=== $nom ===" -ForegroundColor Cyan }

if ($Etape -in @("tout", "python")) {
  Etape "Environnement Python"
  if (-not (Test-Path $Py)) { py -3.12 -m venv $Env_ }
  & $Py -m pip install --upgrade pip wheel
  & $Py -m pip install torch torchaudio --index-url https://download.pytorch.org/whl/cu128
  & $Py -m pip install "pipecat-ai[silero,websocket,openai]==1.14.0" httpx huggingface_hub soundfile
  # Qwen3-TTS accéléré par graphes CUDA et en flux (MIT) ; remplace le paquet officiel qwen-tts, trop lent
  & $Py -m pip uninstall -y qwen-tts
  & $Py -m pip install "faster-qwen3-tts==0.5.4"
  # transformers 5.21 casse le chargement du tokenizer de Qwen3-TTS (upsample devenu une propriété)
  & $Py -m pip install "transformers>=5.15.1,<5.21"
  # le module voix lui-même, en mode développement (le code reste dans le dépôt)
  & $Py -m pip install -e $Src
}

if ($Etape -in @("tout", "transcribe")) {
  Etape "transcribe.cpp (Voxtral) avec CUDA"
  $Tc = Join-Path $Voix "transcribe.cpp"
  if (-not (Test-Path $Tc)) { git clone --depth 1 https://github.com/handy-computer/transcribe.cpp $Tc }
  $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
  $vs = & $vswhere -latest -products * -property installationPath
  $cmake = Get-ChildItem -Recurse -Filter cmake.exe (Join-Path $vs "Common7\IDE\CommonExtensions\Microsoft\CMake") | Select-Object -First 1
  if (-not $cmake) { throw "cmake introuvable dans Visual Studio" }
  $build = Join-Path $Tc "build-cuda"
  & $cmake.FullName -S $Tc -B $build -DTRANSCRIBE_BUILD_SHARED=ON -DTRANSCRIBE_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES=86 "-DCMAKE_CUDA_FLAGS=-allow-unsupported-compiler"
  & $cmake.FullName --build $build --config Release --target transcribe -j 8
  & $Py -m pip install -e (Join-Path $Tc "bindings\python")
}

if ($Etape -in @("tout", "modeles")) {
  Etape "Modèles"
  $hf = @'
import sys
from pathlib import Path
from huggingface_hub import hf_hub_download, snapshot_download
m = Path(sys.argv[1])
hf_hub_download("handy-computer/Voxtral-Mini-4B-Realtime-2602-gguf", "Voxtral-Mini-4B-Realtime-2602-Q8_0.gguf", local_dir=m / "voxtral")
snapshot_download("Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice", local_dir=m / "qwen3-tts" / "Qwen3-TTS-12Hz-1.7B-CustomVoice")
'@
  $hf | & $Py - $Modeles
}

Etape "Terminé"
Write-Host "Mesurer la latence : $Py -m orchestra_voix.bench"
Write-Host "Lancer le serveur  : $Py -m orchestra_voix.server"
