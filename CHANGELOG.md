# Historial de versiones

Formato inspirado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/).
Las fechas son de la sesión de cierre correspondiente (rama `native-iced` salvo
indicación).

## [0.7.1] — 2026-10-29

Versión del **modo Reescritura**: la IA deja de limitarse a arreglar los «digo X»
y pasa a devolver el turno entero bien escrito, si se le pide. Sin cambios en el
formato de la config: un `config.json` antiguo se lee tal cual (ver «Cambiado»).

### Añadido

- **Modo «Reescritura»** (`AiMode::Reescritura`). La IA recibe el turno completo
  y devuelve ese mismo texto limpio y puntuado: sin muletillas, sin
  repeticiones y sin las auto-correcciones que dijiste en voz alta («me duele la
  cabeza y bueno el otro día allá por el 84… digo una experiencia asombrosa» →
  una sola frase bien escrita, con el 84 y sin «digo»). Antes la IA solo actuaba
  cuando veía una corrección; ahora, en este modo, actúa en cada turno.
- **Tres modos en Ajustes → Corrección IA**: `Desactivada` / `Corrección` /
  `Reescritura` (`AiMode::ALL`). Cada uno con su explicación de una línea. Con
  el modo `Corrección` todo funciona como en la 0.7.0.
- **Qwen3-4B-Instruct-2507 en la lista de modelos descargables** (~2,4 GB,
  `Qwen3-4B-Instruct-2507-Q4_K_M.gguf`), marcado como **el mejor para
  reescribir**. Los tres pequeños siguen ahí igual. Si eliges Reescritura con
  uno de los pequeños, Ajustes lo avisa: devuelven el texto sin tocar o una
  lista de reglas.
- **Aviso al reescribir**: el panel de la corrección dice «Reescrito IA: «antes»
  → «después»» en vez de presentarlo como una auto-corrección, y la fila de
  estado nombra el modo (`Reescritura IA`).

### Cambiado

- **El guard de números se aplica ANTES del modelo** (`precondition_numeric`).
  Era el fallo de fondo: si el turno llegaba con «83 digo 84», el modelo escribía
  «83 o el 84» (se inventa la disyuntiva) y se perdía una corrección que el
  código sabía hacer bien. Ahora el guard determinista deja el turno en «84» y
  al modelo solo le llega el texto ya arreglado: sale limpio, sin «digo» y algo
  más rápido (0,43 s → 0,37 s).
- **Listón de recorte por modo** (`min_words_ratio`): la protección contra
  resúmenes exige el 60 % de las palabras en `Corrección` y baja al **50 %** en
  `Reescritura`, porque quitar muletillas recorta de verdad. Por debajo sigue
  oliendo a resumen y se tira.
- **`config.json` compatible**: el campo nuevo `ai_mode` se deriva del antiguo
  `ai_correction_enabled` (`true` → `Correccion`, `false` → `Off`), y
  `ai_correction_enabled()` pasa a significar «modo distinto de Off». Quien
  venga de la 0.7.0 no nota nada.

### Corregido

- **Se tiraba la corrección numérica buena** cuando la respuesta del modelo
  traía meta-lengua. Ahora el texto que se escribe es el mejor de los dos (el
  dictado pasado por el guard o la respuesta limpia del modelo) y los fallbacks
  vuelven a ese texto, no al bruto: un «83 digo 84» ya no se queda con las dos
  cifras ni con la marca.

### Verificado

- `cargo test`: **352 correctos, 0 fallos, 1 ignorado** (eran 345 en la 0.7.0).
  `cargo check` sin errores, con los dos avisos conocidos de `src/hotkey.rs`.
- Prueba real contra `llama-server` con Qwen3-4B-Instruct-2507 (RTX 4070): la
  frase del ejemplo sale sin «digo», con el «84» y en **0,37–0,53 s por turno**.
  Con el guard delante, «83 digo 84» → «allá por el 84» en 166 ms; sin el guard,
  el 4B escribe «83 o el 84» y el fallo se reproduce.
- Límites conocidos: en Reescritura el modelo **no debe resumir** (el prompt lo
  prohíbe y el listón del 50 % lo vigila) y **puede inventar disyuntivas** con
  los números que no vienen del guard (dichos con palabras: «tres mil», «cuarenta
  y ocho»), porque la protección numérica solo entiende dígitos y la lista corta.

## [0.7.0] — 2026-10-29

Versión de **comodidad**: el modelo que corrige ya no se busca a mano. Se elige
en un desplegable dentro de Ajustes y la app lo baja de Hugging Face sola. Sin
cambios en el formato de datos ni en la config: quien venga de la 0.6.0 conserva
todo lo suyo.

### Añadido

- **El modelo de IA se elige en un desplegable.** El campo de texto «Modelo IA
  local» pasa a ser una lista con tres modelos pequeños probados: Qwen2.5-0.5B
  (~491 MB, el recomendado), Llama 3.2 1B (~808 MB) y Qwen2.5-1.5B (~1,1 GB,
  el más fiel). Al lado está **«Otro (repo o URL de Hugging Face)…»**, que abre
  un campo para pegar `owner/repo` o la dirección de un `.gguf`.
- **Botón «Descargar».** Si el modelo elegido no está en el disco, el bloque
  enseña el botón con lo que ocupa y lo baja a
  `%APPDATA%\whispr-open\models-llm\` con progreso («Descargando… 123 MB de
  491 MB (25 %)»); si el servidor no dice el tamaño, lo dice tal cual en vez de
  inventarse un total. Al terminar avisa («Listo: … ya está en models-llm.») y
  vuelve a arrancar el servidor de IA por el camino de siempre. Si el fichero
  ya estaba, no ofrece descargarlo.
- `src/llm.rs`: entender una referencia de Hugging Face (`parse_hf_ref`), mirar
  los ficheros del repo y quedarse con el **GGUF** (`pick_gguf`: prefiere
  `q4_k_m` y descarta lo que no es GGUF) y bajar el fichero comprobando su marca
  GGUF (`ensure_llm_model`). Cuatro pruebas nuevas cubren esas piezas.

### Cambiado

- **Escribir a mano es la excepción.** El campo libre solo aparece detrás de
  «Otro»; el camino normal es elegir de la lista.
- **La descarga no toca el dictado.** El progreso y los fallos viven en el
  bloque de Ajustes: una red caída deja un aviso ahí y el dictado sigue igual.
- **Limpieza del repo**: fuera el cuaderno de trabajo y los informes sueltos que
  no eran documentación del producto, y enlaces arreglados (`AGENTS.md` al día).

### Corregido

- **GGUF dentro de una subcarpeta del repo.** Se apuntaba la ruta del fichero
  dentro del repo, y la app busca por nombre de fichero: el modelo recién
  bajado no se habría encontrado. Ahora se guarda el nombre real del fichero que
  queda en `models-llm/`.

### Verificado

- `cargo test`: 345 correctos, 0 fallos, 1 ignorado (eran 341 antes de esta
  versión). `cargo check` sin errores, con los dos avisos conocidos de
  `src/hotkey.rs`.
- Llamada real a Hugging Face: `Qwen/Qwen2.5-0.5B-Instruct-GGUF` resuelve a
  `…/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf`, y con ese mismo código se
  bajó un GGUF de prueba (progreso, redirección del servidor y marca GGUF
  comprobada).
- Límites conocidos, dichos claros en la interfaz: **no se reanudan descargas a
  medias** (cada intento empieza de cero) y **solo se aceptan GGUF públicos**;
  si el repo no trae ninguno, se avisa y no se baja nada.

## [0.6.0] — 2026-10-29

Versión de **peso y port**: el instalador pasa de 549 MB a 15 MB y el port a
Linux se puede probar de verdad. Sin cambios en el formato de datos ni en la
config: actualizar por encima de una 0.5.0 no pide reconfigurar nada.

### Cambiado

- **El instalador ya no pesa medio giga.** El base es **CPU-only** y pesa
  ~15,4 MB: lleva el exe de CPU y deja fuera `vendor\llama\cuda\` (656 MB de
  DLLs de NVIDIA). La GPU se sirve en un **pack CUDA aparte** (~522 MB) que se
  instala encima del base y añade `silb-cuda.exe`.
- **El pack CUDA comprueba el runtime antes de copiar.** `silb-cuda.exe` importa
  `cudart64_12.dll` y `cublas64_12.dll`: si el equipo no los tiene, el
  instalador del pack se para, no toca nada y remite al instalador CPU. Antes
  se podía dejar instalado un exe que Windows no llegaba a arrancar.
- **Actualizar del pack al base conserva `cuda\`** (el servidor LLM sigue
  usando la GPU aunque el dictado pase a CPU); el desinstalador sí borra esos
  ficheros, así que no queda basura permanente.

### Añadido

- `scripts/linux-check.sh`: comprobación completa en Linux (`cargo check`,
  tests, build release, tamaño y dependencias del binario) que deja informe en
  `docs/VERIFICACION-LINUX-<fecha>.txt`. Si no hay binario, ahora cuenta como
  fallo en vez de cerrar en verde.
- Guía de prueba del port en `docs/PORT-LINUX/`: qué hay que comprobar en cada
  apartado y qué se considera correcto.

### Corregido

- **Selector de micrófono en Linux**: el host de audio lista todo el zoo de
  plugins de ALSA (`null`, `lavrate`, `speexrate`, `jack`, `oss`, `upmix`,
  `vdownmix`, `usbstream`…) como si fueran micrófonos. Ahora solo aparecen la
  entrada por defecto (`default`, `pipewire`, `pulse`) y los PCM de tarjeta
  (`hw:`, `plughw:`, `sysdefault:`, `front:`).
- **El log dice qué micrófono se abre** (`dispositivo de entrada: «…»`): antes
  no había forma de saber si dictabas con el micro interno o con el USB.
- **Los scripts de Linux apuntaban a un binario inexistente**
  (`target/release/whispr-open`); el binario real es `silb`.
- **Rutas de modelos de llama.cpp en Linux**: ya no se listan `C:/` y `D:/`
  como si fueran carpetas locales.
- **Burbuja de dictado en Wayland**: se queda en el workspace donde arranca. No
  se puede fijar desde la app; queda documentada la regla del compositor
  (Hyprland: `float` + `pin`) en `docs/PORT-LINUX/CODIGO-UI.md`.

### Verificado

- `cargo test`: 341 correctos en Windows, 0 fallos (1 ignorado); suite completa
  también en verde en Linux (Ryzen 5 PRO 5650U, Arch).
- Rendimiento con el modelo `small` y `beam=5`: en CPU (6 hilos) 0,21–0,63 de
  tiempo real según la duración del audio; en GPU (RTX 4070) 0,06–0,21, es
  decir **3–4× más rápido**.

## [0.5.0] — 2026-10-26

Primera versión que **compila y arranca en Linux** además de Windows. Sin
cambios de formato de datos: la config y los modelos siguen en
`%APPDATA%\whispr-open` (Windows) / `~/.local/share/whispr-open` (Linux).

### Añadido

- **Port a Linux integrado** (venía de las ramas `feature/port-linux-*` del
  3-4 de octubre, nunca mergeadas). Lo que se comporta distinto por plataforma:
  - **Atajo global**: en Windows sigue `global-hotkey`; en Wayland/Hyprland se
    usa el socket de `hyprctl` (`docs/PORT-LINUX/HOTKEY-HYPR.md`).
  - **Inyección en la app activa**: Windows con `enigo` + portapapeles; en
    Linux con `wl-copy`/`wtype`/`ydotool` según `inject_backend`.
  - **Instancia única**: mutex con nombre en Windows, `flock` en
    `$XDG_RUNTIME_DIR/whispr-open/instance.lock` en Linux.
  - **Server LLM local**: `llama-server.exe` en Windows, `llama-server` en
    Linux; en ambos casos el proceso se termina al salir (SIGTERM + SIGKILL de
    rescate en Linux, `TerminateProcess` verificado por imagen en Windows).
  - **Bandeja**: el tray es de Windows; en Linux la app funciona sin bandeja
    (ventana + atajo + Salir en la UI) porque Hyprland no trae host
    StatusNotifier.
  - **Burbuja flotante**: sin `WS_EX_NOACTIVATE` en Linux; el resto del
    comportamiento es el mismo.

### Notas

- En Linux no hay instalador: se compila con `scripts/build-linux.sh` y se
  lanza con `scripts/run-linux.sh`.
- El runtime de Linux está verificado a nivel de compilación (`cargo check` y
  `cargo test` en Ubuntu), **no** todavía en una sesión Wayland real: la
  primera prueba de micrófono + inyección en Hyprland queda pendiente.

## [0.4.0] — 2026-10-19

Primera versión con el producto ya renombrado a **Silb** y el repo listo para
publicarse. Sin cambios de formato de datos: la config y los modelos siguen en
`%APPDATA%\whispr-open`.

### Añadido

- **Renombrado a Silb**: binario `silb.exe`, instalador `silb-<ver>-setup.exe`,
  icono y textos de la app. El paquete y el directorio de datos conservan
  `whispr-open` para no dejar sin ajustes a quien ya la tiene instalada.
- **Rediseño de UI desde Figma** (fase 1: tokens y frame Dictado; fase 2:
  frame Ajustes) y retícula única en las cinco pestañas.
- **GIF animado en la burbuja flotante** (codec gif, sin arrastrar
  rav1e/avif al binario).
- **Catálogo local de modelos LLM** y rename de proveedor en Ajustes.
- **Server LLM integrado en el paquete** (`vendor/llama`): la app lo lanza y
  lo cierra sola, sin rutas manuales.
- **Secciones en la ventana principal**, pestañas compartidas y avisos.
- **Build CUDA opcional**: `cargo build --release --features cuda`. Por
  defecto compila en CPU (MSVC + CMake), sin toolkit CUDA ni GPU.

### Cambiado

- `Cargo.lock` trackeado: builds reproducibles.
- Conteo de tests corregido con medición real sobre el tip: **336 passed,
  0 failed, 3 ignored**.
- Documentación sin rutas ni informes internos de sesión.

### Corregido

- El guard numérico caza el numeral viejo en frases naturales, y la
  reconstrucción es el texto que se inyecta.
- Los assets de personalización se copian al directorio de datos.
- El log rota por tamaño y deja de registrar el ruido del arrastre.
- Fallback al micrófono por defecto si el configurado desaparece.
- Se conserva el modelo local cuando falla su carga (bucle DNS).
- Eliminado el `expect` frágil de la imagen de fondo.

### Notas de empaquetado

- El instalador incluye el server LLM (`vendor/llama`, ~44 MB en CPU; el pack
  CUDA opcional añade ~656 MB). Los binarios no viajan en el repo: se copian
  con `packaging/vendor-llama.sh`.

## [0.3.0] — 2026-09-11

Primera release pensada para gente de fuera: instalador, icono de producto y
personalización completa dentro del binario.

### Añadido

- **Instalador Inno Setup** (`packaging/whispr-open.iss`): instala en
  `%LOCALAPPDATA%\Programs\WhisprOpen` **sin administrador**, con acceso
  directo (menú Inicio + escritorio opcional), entrada correcta en «Agregar o
  quitar programas» y desinstalador que pregunta si borrar config y modelos.
  Salida: `whispr-open-0.3.0-setup.exe` (~23 MiB con LZMA2/ultra64).
- **Icono multirresolución** (16/24/32/48/64/128/256, `#7C6CFF`) generado por
  código (`assets/build_icon.py`, invocado desde `build.rs`) e incrustado en
  el `.exe` y en los accesos directos.
- **Personalización completa** (Ajustes): imagen de fondo propia, sprite o
  animación por frames en la burbuja, paleta editable con presets y por campo,
  y Ajustes reorganizados por secciones con pestañas.
- **Variante portable**: el propio `target\release\whispr-open.exe`
  autocontenido, sin instalación.

### Cambiado

- Versión 0.2.0 → 0.3.0 (cierre de las fases R6+R7 del roadmap).
- La documentación de cierre queda en `INFORME-CIERRE.md` y
  `docs/EMPAQUETADO.md` (medidas reales de tamaño y RAM).

## [0.2.0] — 2026-09-11

Reescritura a **escritorio nativo** (Rust + Iced + whisper.cpp). Adiós web.

### Añadido

- Corrección IA completa: server LLM nativo autogestionado (la app lo lanza y
  lo cierra), guard anti-truncación, proveedores **Local** (llama.cpp) /
  **Custom** (endpoint OpenAI-compatible) / **Cloud** (OpenAI), API key en el
  Credential Manager de Windows, chip de estado y opciones explicadas.
- Nota editable antes de inyección, con confirmación de copiado y borrado
  deshace-able.
- Robustez a prueba de fallos: descarga de modelos con reintentos y reanudación,
  micrófono que muere a mitad de grabación, inyección sin ventana con foco
  (queda en portapapeles, nunca se pierde), guard de instancia única.
- Contraste AA medido (`src/theme.rs`), pie de confianza y descarga con MB y
  tiempo restante.
- Licencia MIT, `docs/ARQUITECTURA.md` para quien llega nuevo, guía de
  usuario ampliada y doc-comments en español en funciones y tipos públicos.

## [0.1.0] — 2026-09-02 (rama `main`)

Prototipo web (React + Tauri v2 + Whisper WASM): 40 tests. Queda archivado en
el repo privado `whispr-open`, fuera de este.

[0.4.0]: https://github.com/JuanFrancisco-Mateo/silb/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/JuanFrancisco-Mateo/silb/releases/tag/v0.3.0
[0.2.0]: https://github.com/JuanFrancisco-Mateo/silb/compare/v0.1.0...v0.3.0
[0.1.0]: https://github.com/JuanFrancisco-Mateo/silb/releases/tag/v0.1.0
