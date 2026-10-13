# Historial de versiones

Formato inspirado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/).
Las fechas son de la sesión de cierre correspondiente (rama `native-iced` salvo
indicación).

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
la rama `main` como referencia; su hoja de ruta está en
[`docs/ROADMAP-LEGADO-WEB.md`](docs/ROADMAP-LEGADO-WEB.md).

[0.3.0]: https://github.com/JuanFrancisco-Mateo/whispr-open/releases/tag/v0.3.0
[0.2.0]: https://github.com/JuanFrancisco-Mateo/whispr-open/compare/v0.1.0...v0.3.0
[0.1.0]: https://github.com/JuanFrancisco-Mateo/whispr-open/releases/tag/v0.1.0
