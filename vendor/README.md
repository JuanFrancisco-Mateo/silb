# vendor/ — lo que VIAJA con la app

Nada de aquí va a git (binarios de cientos de MB, ver `.gitignore`). Se copia
desde el build local con un script.

## `vendor/llama/` — el server LLM (obligatorio para la corrección IA)

Mismo layout que la instalación de llama.cpp (por defecto `D:\LlamaCPP`):
`llama-server.exe` con sus DLL hermanas (`llama-server-impl.dll`,
`llama-common.dll`, `llama.dll`, `ggml*.dll`, `libomp140.x86_64.dll`). La app
lo busca como PRIMER candidato en `exe_dir/vendor/llama/llama-server.exe`
(`llm::resolve_server_path`): el usuario no teclea ninguna ruta.

Copiarlo (idempotente, verifica tamaño + sha256 de cada fichero):

    bash packaging/vendor-llama.sh                 # sin pack CUDA (~44 MB)
    bash packaging/vendor-llama.sh --cuda          # con pack CUDA (~700 MB)
    bash packaging/vendor-llama.sh --cuda D:/LlamaCPP   # otro origen

Tamaños medidos (build CUDA 13 de esta máquina, 14/10/2026):

| pack | contenido | tamaño |
|------|-----------|--------|
| base | server + impl/llama/ggml DLLs (CPU) | ~44 MB |
| cuda | `cuda/`: `ggml-cuda.dll` (148 MB) + `cublasLt64_13.dll` (459 MB) + `cublas64_13.dll` (50 MB) + `cudart64_13.dll` (0,5 MB) | ~656 MB |

## `vendor/llama/cuda/` — runtime CUDA (opcional)

Pack opcional: si el directorio existe **y** la máquina tiene driver NVIDIA
(`%SystemRoot%\System32\nvcuda.dll`), la app arranca el server con ese
directorio como cwd — el cargador de backends de ggml busca `ggml-*.dll` en el
cwd y en el directorio del exe, **no** en el PATH (verificado con el binario
real) — y con ese directorio al frente del PATH, para las dependencias de
`ggml-cuda.dll` (cudart/cublas). Sin el pack (o sin driver) el server corre en
CPU, sin avisos.

## `vendor/models/` — modelos GGUF (opcional)

Si algún día el GGUF del corrector viaja con la app, va aquí: es la primera
raíz donde `llm::resolve_model_path` busca el modelo por nombre (después
`%APPDATA%\whispr-open\models`, la misma carpeta donde la app descarga los
modelos de whisper).
