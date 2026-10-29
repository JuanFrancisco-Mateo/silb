# QA 0.6.0 — instaladores (base / pack CUDA) y scripts Linux

**Fecha:** 2026-10-28 · **Rama:** `release/0.6.0`
**Commits revisados:** `63bd6ce` (el pack CUDA sale del instalador base) y
`b4670a2` (linux-check y scripts apuntando a `silb`). Base: `release/0.5.0`.
**Arreglos de esta revisión:** `e46a41b` (runtime CUDA 12 del pack) y
`778cedf` (binario viejo en linux-check).
**Entorno:** Windows 11 26200, rustc 1.92.0, Inno Setup 6.7.3. La parte de Linux
se apoya en la pasada que ya hizo el usuario en el portátil (Arch, rustc
1.99.1: check, check --tests, test y binario de 23 MB OK).
**Alcance:** `packaging/whispr-open.iss`, `scripts/build-linux.sh`,
`scripts/run-linux.sh`, `scripts/linux-check.sh`, `src/llm.rs` y las referencias
al binario viejo.

**Cómo se hizo.** Lectura del código y del `.iss`; compilación del instalador
base tres veces (antes y después del arreglo); compilación de la rama
`/DMyCudaPack` **una vez** en una carpeta de trabajo con `Compression=none`
(786.199.874 B, borrada después) para validar el `#ifdef`; instalador de prueba
con un AppId propio para reproducir la migración v1-con-`cuda\` → v2-base →
desinstalar (nunca sobre la instalación real); `linux-check.sh` en un entorno
aislado con `cargo` simulado y sin `.git`. **No se ejecutó
`scripts/install-smoke.py`** ni ningún instalador sobre la instalación real del
usuario.

---

## Tabla de hallazgos

| # | Hallazgo | Gravedad | Evidencia (comando → salida) | Arreglo propuesto |
|---|---|---|---|---|
| H1 | El pack CUDA instala `silb-cuda.exe`, que **importa** `cudart64_12.dll` y `cublas64_12.dll`. El pack no las trae (`vendor\llama\cuda` es CUDA 13, solo para `llama-server`). Sin el runtime CUDA 12 en el equipo, Windows no crea el proceso: el pack deja una app que no arranca. El instalador 0.5.0 sí lo comprobaba y el refactor perdió el chequeo. | **ALTA** (corregido en `e46a41b`) | `dumpbin /imports packaging/build/silb-cuda.exe` → `cublas64_12.dll`, `cudart64_12.dll`, `nvcuda.dll`; `dumpbin /imports vendor/llama/cuda/ggml-cuda.dll` → `cublas64_13.dll`; `ls vendor/llama/cuda` → `cublas64_13.dll`, `cublasLt64_13.dll`, `cudart64_13.dll`, `ggml-cuda.dll`; `git show 63bd6ce` → `UseCuda` pasó de `FindRuntimeDll('cudart64_12.dll') and FindRuntimeDll('cublas64_12.dll')` a `Result := True` | Restaurado `PrepareToInstall` + `FindRuntimeDll` (`cudart64_12`/`cublas64_12`): aborta antes de copiar y remite al instalador CPU. Doc en `EMPAQUETADO.md`. **Recompilar el pack**: el `.exe` de 547.853.368 B de `packaging/Output` es anterior al arreglo. |
| H2 | Base sobre una instalación con pack CUDA: el `silb.exe` pasa a CPU y `backend.txt` a `cpu`, pero **`vendor\llama\cuda\` (656 MB) se queda**. La app sigue usando ese `cuda\` para el server LLM (no para whisper). No se recupera el espacio hasta desinstalar. Riesgo añadido: en una actualización futura, `llama-server.exe`/DLLs nuevas convivirían con un `ggml-cuda.dll` viejo. | MEDIA | Test sintético v1→v2: tras v2 siguen `install/cuda/big.bin`; `src/llm.rs:1365-1374` (`cuda_dir.is_dir() && nvidia_driver_present()`, no lee `backend.txt`); uninstall sintético → `ls install` = `No such file or directory` | Opción A: `DelTree('{app}\vendor\llama\cuda', ...)` en el base (ssInstall, tras `CloseApplications`). Opción B: documentar que se conserva hasta desinstalar. **No aplicado**: borrar 656 MB es decisión de producto. |
| H3 | `backend.txt` no lo lee nadie: solo lo escribe el `.iss`. `backend=cpu` con el exe CPU no cambia el comportamiento real y la UI no muestra backend, así que no hay mentira visible; el fichero es metadato muerto. | BAJA | `grep -rn "backend.txt" src` → 0 resultados; solo aparece en `packaging/whispr-open.iss` (154, 156, 275). `grep -ri "cuda\|gpu" src/ui` → 0 | Leerlo en la app (log/Ajustes) o dejar de escribirlo. No aplicado: toca app/UI. |
| H4 | El pack avisa de que no hay GPU NVIDIA pero **no bloquea**; `silb-cuda.exe` también importa `nvcuda.dll` (driver). Si el usuario ignora el aviso, instala una app que no arranca. | BAJA | Imports de H1 (`nvcuda.dll`); `[Code]` → `MsgBox(..., mbError, MB_OK)` y sigue | Abortar también sin `nvcuda.dll`, o cambiar el texto a «instalar de todos modos». No aplicado: el aviso lo diseñó el autor. |
| H5 | El base empaqueta `target\release\silb.exe` sin fijar que sea el build CPU. El build `--features cuda` deja el exe CUDA (49,8 MB) en esa misma ruta; si es el último, el base sale con un exe que necesita `cuda\` y no arranca en equipo limpio. Hoy el artefacto es correcto. | BAJA (proceso) | `packaging/build/cuda.log` → el build CUDA compiló en `target`; `dumpbin /imports target/release/silb.exe \| grep -i cuda` → vacío; base 15.407.267 B = payload CPU | Documentado el orden de build en `EMPAQUETADO.md` (commit `e46a41b`). Opcional: fijar `packaging\build\silb-cpu.exe`. |
| H6 | La rama es `release/0.6.0` pero `Cargo.toml`, el default del `.iss` y los artefactos siguen en **0.5.0**. | BAJA (release) | `Cargo.toml:3` → `version = "0.5.0"`; salida → `silb-0.5.0-setup.exe` | Subir `Cargo.toml` a 0.6.0 y compilar con `WHISPR_VERSION=0.6.0`; rehacer los dos artefactos. |
| H7 | Documentación desfasada: cabecera del `.iss` («~30 MB comprimido», «~35 MB con los dos builds», default 0.4.0), `EMPAQUETADO.md` §4 («Actual: 0.4.0») y §1.2.1, y `RELEASE-CHECKLIST.md` (0.4.0, un solo artefacto). | BAJA | Lectura de los tres ficheros | Actualizar cabecera y §4/§1.2.1; checklist de 0.6 con base + pack y requisito CUDA 12. |
| H8 | `linux-check.sh` mide `target/release/whispr-open` si falta `silb`: es justo el binario viejo que el commit declaró obsoleto, y el informe lo presenta como el build actual. | BAJA (corregido en `778cedf`) | Antes: sandbox con solo el binario viejo → `binario : target/release/whispr-open 1.0K`. Después: solo viejo → `no encontrado`; con `silb` → `target/release/silb 1.0K` | Quitado el fallback: `BIN=target/release/silb`; si no existe, «no encontrado». |
| H9 | La cabecera de `linux-check.sh` dice «parando al primer fallo de compilación», pero sigue y acumula fallos; el mensaje «ver detalle arriba» es falso en stdout (el detalle va solo al informe). | BAJA | `FAKE_CARGO_EXIT=1` → salen los 3 `FALLO` y `EXIT=3`; stdout sin detalle; informe con el detalle | Corregir la cabecera a «sigue y cuenta fallos» y el mensaje a «ver informe». No aplicado. |
| H10 | Con `SKIP_BUILD=1` y sin binario, el informe dice «RESULTADO: TODO OK» y sale 0 sin haber medido nada. | BAJA | Sin binarios → `binario : no encontrado (¿build saltado?)` + `RESULTADO: TODO OK`, `EXIT=0` | Marcar el informe como «sin medidas» o devolver != 0 si falta el binario. No aplicado (skip es explícito). |

---

## Detalle por punto

### a) `Excludes: "cuda"` con `recursesubdirs`

**Sin hallazgo.** El compilador de Inno 6.7.3 quita la **carpeta** `cuda\`
entera, no solo un fichero llamado `cuda`:

```
ISCC.exe <copia de whispr-open.iss con OutputDir=out>   (log sin /Q)
  → Successful compile (18,172 sec)
  → salida = 15.407.257 B
grep -a "Compressing:" <log de ISCC> | awk -F'\\' '{print $NF}' | sort
  → 26 entradas: silb.exe, whispr-open.ico y 24 ficheros de vendor\llama\
  → 0 entradas con "cuda"
```

La lista de 24 ficheros coincide con el inventario de `vendor/llama` sin `cuda/`
(todos los `ggml-cpu-*.dll`, `ggml*.dll`, `llama*.dll`, `mtmd.dll`, `libomp140`,
etc.). Los 656 MB de `vendor/llama/cuda` no salen ni en el registro ni en el
tamaño. La duda que plantea la documentación de Inno («un patrón sin `\` inicial
se compara con el final del path») queda resuelta a favor de lo que se buscaba:
`cuda` excluye la subcarpeta entera.

La rama `/DMyCudaPack` compila y añade `vendor\llama\cuda\*` aparte; el `.iss` ya
se para en la compilación si falta `cuda\ggml-cuda.dll`.

### b) Migración 0.5.0 (CUDA) → 0.6.0 base

Reproducido con un instalador de prueba (AppId `{A1B2…}`, carpeta temporal, nunca
el AppId real):

```
v1: src\* con src\cuda\big.bin        → install\base.txt, install\cuda\big.bin, unins000.*
v2: misma AppId, Excludes: "cuda"     → siguen install\base.txt y install\cuda\big.bin
unins000.exe /VERYSILENT              → ls install: No such file or directory
reg query ... QA-Silb-Migracion       → 0 coincidencias
```

- **Funciona igual:** no se rompe nada. El `silb.exe` que queda es el de CPU (no
  usa CUDA), así que whisper transcribe en CPU. El `cuda\` que sobra **sí lo
  sigue usando el servidor de IA**: `launch_server` decide por
  `cuda_dir.is_dir() && nvidia_driver_present()` (`src/llm.rs:1365-1374`), no por
  `backend.txt`. En una máquina sin `nvcuda.dll` lo ignora y apunta «pack CUDA
  ignorado: no hay driver NVIDIA», sin romperse.
- **Espacio:** los 656 MB se quedan hasta desinstalar, pero **el desinstalador sí
  los borra**: Inno conserva el registro de desinstalación de la versión anterior
  al actualizar encima y la prueba con el instalador de mentira lo confirma. No
  se pierde nada para siempre; sí hay 656 MB retenidos mientras la app está
  instalada.
- **`backend.txt`:** pasa a `cpu`, pero no lo lee nadie (H3). No hay nada raro a
  la vista porque la interfaz no enseña el backend.
- **Riesgo estructural:** el base reemplaza `llama-server.exe`/DLL de CPU pero no
  toca `cuda\`; una actualización futura del vendor dejaría un `ggml-cuda.dll`
  viejo con un servidor nuevo. Hoy los dos packs salen del mismo `vendor/llama`
  (14/10), así que todavía no ocurre.

### c) Pack CUDA encima del base con Silb abierto

**Ningún defecto en el `.iss`; es un comportamiento documentado.** Inno 6.7.3 usa
`CloseApplications` por defecto (`yes`) y el Restart Manager de Windows: al
detectar `silb.exe` en uso, en modo interactivo se para en «Preparando la
instalación», lista la app y ofrece cerrarla; en silencioso la cierra solo
(`scripts/install-test.bat` ya pasa `/CLOSEAPPLICATIONS`). Si te niegas, el
instalador no puede reemplazar `silb.exe` y termina con error.
`RestartApplications` también es `yes` por defecto, pero Silb **no** llama a
`RegisterApplicationRestart` (`grep` en `src` → 0), así que el Restart Manager no
la vuelve a abrir: la abre la entrada `[Run]` del final (con `skipifsilent`, o
sea, no en silencioso).

No se ha probado sobre la instalación real (estaba prohibido usar
`install-smoke.py`). Queda documentado en `EMPAQUETADO.md` §7.2 (commit
`e46a41b`).

### d) ¿Algo en `src/` asume el exe CUDA?

**Ni se rompe ni engaña a la vista.** No existe `backend` en `Config` (solo
`inject_backend`, el de la escritura en Linux); `src/ui` no menciona CUDA ni GPU;
el único que leería el backend sería `backend.txt`, que no tiene lector. El exe
de CPU (`target/release/silb.exe`, 23.675.392 B) no usa CUDA, y el
`llama-server.exe`/`llama-server-impl.dll` que viajan con la app tampoco: el
servidor arranca en CPU sin `cuda\`. La app se degrada sola por diseño: sin
`cuda\` la carpeta de trabajo es la del exe y no toca el PATH; con `cuda\` y
driver usa la GPU solo para la IA. El `backend.txt` muerto es H3.

### e) `linux-check.sh`

La mecánica está bien: cuenta fallos (`EXIT=3` con tres pasos fallidos), rehace
el informe (dos ejecuciones → una sola cabecera `VERIFICACIÓN LINUX`), funciona
sin `.git` (`Commit: (sin git)`) y con `SKIP_BUILD=1`. Los defectos son del
informe, no de la ejecución: H8 (medía el binario viejo; corregido en `778cedf`),
H9 (la cabecera promete parar y no para; «ver detalle arriba» no vale para
stdout) y H10 (TODO OK sin binario). `step` no se para a propósito, así que el
recuento y el código de salida cuadran con lo que hace, no con lo que dice la
cabecera.

### f) Referencias a `target/release/whispr-open`

**Ninguna viva.** `scripts/linux-check.sh:55` era la última y se corrigió en
`778cedf` (H8). `CHANGELOG.md:103` es histórica (describe la versión portable
anterior al cambio de nombre) y `EMPAQUETADO.md` explica el nombre viejo. No hay
integración continua: `.github` solo tiene plantillas de issues/PR, sin
workflows. `build-linux.sh`, `run-linux.sh` y `measure-release-profile.sh` ya
usan `silb`.

---

## Evidencia reproducible (resumen)

```text
# 1) Base: payload real (copia del .iss solo con OutputDir=out)
ISCC.exe <copia del .iss>
  → Successful compile (18,172 sec) · silb-0.5.0-setup.exe 15.407.257 B
  → 26 "Compressing", 24 de vendor/llama, 0 con "cuda"

# 2) Imports (dumpbin 14.29, el del VS2019 documentado en scripts/)
dumpbin /imports packaging/build/silb-cuda.exe | grep -iE "cublas|cudart|nvcuda"
  → cublas64_12.dll, cudart64_12.dll, nvcuda.dll
dumpbin /imports vendor/llama/cuda/ggml-cuda.dll | grep -i cublas
  → cublas64_13.dll
dumpbin /imports vendor/llama/llama-server-impl.dll | grep -iE "cublas|cudart"
  → (vacío: el server vendado no depende de CUDA)
dumpbin /imports target/release/silb.exe | grep -iE "cublas|cudart"
  → (vacío: el exe CPU no depende de CUDA)

# 3) Migración sintética (AppId propio, directorio temporal)
v1 con cuda/big.bin → v2 misma AppId sin cuda → sigue cuda/big.bin
unins000.exe /VERYSILENT → install eliminado, ARP 0 coincidencias

# 4) linux-check.sh en sandbox (cargo simulado, sin .git)
todos OK + SKIP_BUILD=1       → EXIT=0, "Commit: (sin git)", TODO OK
FAKE_CARGO_EXIT=1             → EXIT=3, 3 FALLO (no para en el primero)
dos ejecuciones               → 1 sola cabecera (informe regenerado)
sin binarios                  → "no encontrado" + TODO OK, EXIT=0
solo whispr-open (antes)      → "binario: target/release/whispr-open"
solo whispr-open (778cedf)    → "no encontrado"
con silb (778cedf)            → "binario: target/release/silb 1.0K"
```

## No verificado (y por qué)

- `scripts/install-smoke.py`: la tarea lo prohibía (desinstala la instalación
  real).
- Instalación real del base o del pack, actualización encima y desinstalación con
  el AppId real: no se ha tocado la instalación del usuario; la migración se
  probó con un AppId inventado.
- Arranque del exe CUDA sin runtime: se deduce de la tabla de dependencias (el
  cargador de Windows pide esas DLL antes de llegar a `main`); no se lanzó para no
  dejar una ventana modal de Windows abierta.
- La comprobación de `PrepareToInstall`: compila en las dos ramas (base y
  `/DMyCudaPack`), pero no se ejecutó ninguna instalación; su comportamiento en
  marcha es el que documenta Inno (texto no vacío → se para en «Preparando la
  instalación» y sale con código propio).
- El flujo de micrófono e interfaz y la ejecución nativa en Linux: fuera de
  alcance; se usa la línea base del usuario.

---

## Resumen (5 líneas)

1. **El base de 0.6 se puede publicar**: 15,4 MB, contenido comprobado sin
   `cuda\`, exe de CPU sin dependencias de CUDA y `llama-server` de la app sin
   dependencias de CUDA; faltan subir la versión a 0.6.0 y probar la instalación
   de verdad.
2. **El pack CUDA no se podía publicar**: `silb-cuda.exe` exige `cudart64_12` /
   `cublas64_12` y el pack trae CUDA 13; la comprobación de 0.5.0 se había
   perdido. Corregido en `e46a41b`; hay que **recompilar el pack** para que lo
   lleve.
3. **La migración 0.5→0.6 base no rompe**, pero deja 656 MB de `cuda\` y la IA
   sigue en GPU; el desinstalador sí borra esos ficheros (probado).
4. **`backend.txt` es un dato muerto** (nadie lo lee); la interfaz no engaña
   porque no enseña el backend, pero el trabajo del instalador no sirve de nada.
5. **Linux:** los scripts y el binario `silb` están bien; H8 corregido en
   `778cedf` y quedan H9/H10, dos detalles del informe que no bloquean la
   release.

**¿Se puede publicar la 0.6 tal como está?** El base sí, después de subir la
versión y recompilarlo. El pack CUDA **no** hasta recompilarlo con el arreglo
`e46a41b` (y, aun así, solo sirve en equipos con el runtime CUDA 12; el arreglo de
raíz es recompilar `silb-cuda.exe` contra CUDA 13 o meter el runtime 12 dentro).
