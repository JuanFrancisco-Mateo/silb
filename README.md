# Whispr Open

Dictado por voz **local** y de escritorio nativo (Windows). Alternativa
open-source al flujo de Wispr Flow (wisprflow.ai): hablas, sueltas el atajo y
el texto se escribe donde esté tu cursor.

- **UI nativa**: Iced (renderizado directo, sin webview, sin HTML/CSS/JS).
- **Transcripción local**: whisper.cpp vía `whisper-rs` (modelos GGML,
  tiny/base/small). Nada sale de tu máquina.
- **Atajo global** Ctrl+Shift+D (configurable en `config.json` → `shortcut`):
  pulsa para grabar, suelta para transcribir.
- **Inyección en la app activa**: al terminar, el texto se escribe en la
  ventana con foco (Slack, Notion, editor…) vía portapapeles + Ctrl+V.
  Formato (pegar/teclear), retardo y filtro por título de ventana
  configurables en `config.json`.
- **Post-procesado local**: normalización de repeticiones y expresiones de
  relleno antes de la inyección; las reglas son fijas y ya no hay una sección
  de configuración de muletillas en la interfaz.
- **Diccionario personal** (jerga, siglas, nombres propios) persistente.
- **Rectificaciones habladas**: "no, quería decir X" corrige el último turno.

## Requisitos (solo para compilar)

- Rust MSVC toolchain (Windows) + Visual Studio Build Tools (MSVC, cmake) —
  whisper-rs compila whisper.cpp en C desde el primer build.
- Windows 10/11. El binario no necesita nada instalado para ejecutarse.

## Compilar y ejecutar

Desde Windows (PowerShell/cmd):

```powershell
cd E:\AI\FREEBUFF
C:\Users\usuario\.cargo\bin\cargo.exe run --release
```

Desde WSL (la ruta del cargo de Windows es obligatoria):

```bash
cmd.exe /c "cd /d E:\AI\FREEBUFF && C:\Users\usuario\.cargo\bin\cargo.exe run --release"
```

La primera compilación tarda 5–10 min (iced + whisper.cpp). El binario queda en
`target\release\whispr-open.exe`.

## Uso

1. Arranca la app. La primera vez que dictes con un modelo, se descarga solo
   (~75 MB tiny / ~142 MB base / ~466 MB small) a `%APPDATA%\whispr-open\models`.
2. Pulsa **Ctrl+Shift+D** (o el botón ● Grabar), habla, y vuelve a pulsar para
   transcribir.
3. Si "Autoinyectar" está activo, el texto se escribe en la app que tenga el foco.
   Si no, lo verás en la ventana y puedes usar "Escribir en app" / "Copiar".
4. En Configuración: modelo de Whisper, apariencia, corrección IA y diccionario.

Modelo de transcripción: `es` por defecto (los modelos base de whisper.cpp son
multilingües; el idioma se puede cambiar a `auto`, `es`, `en`, `fr`, `de`, `it`
o `pt` desde Ajustes). El audio se entrega siempre a Whisper en 16 kHz.

## Configuración avanzada (`%APPDATA%\whispr-open\config.json`)

Además de lo que se ajusta desde la UI (Ajustes), el archivo de configuración
permite:

| Clave | Valores | Default |
|---|---|---|
| `shortcut` | atajo global de dictado: `"Ctrl+Shift+D"`, `"Alt+Space"`, `"Ctrl+Alt+F12"`, `"Ctrl+Shift+K"`… | `"Ctrl+Shift+D"` |
| `inject_format` | `"paste"` (portapapeles + Ctrl+V) o `"type"` (tecleo directo, sin tocar el portapapeles) | `"paste"` |
| `inject_delay_ms` | retardo en ms antes de pegar/teclear (`0` = sin retardo) | `40` |
| `inject_window_filter` | solo inyecta si el título de la ventana activa lo contiene (substring, sin distinguir mayúsculas) | (sin filtro: cualquier app) |
| `correction_provider` | `default`, `cloud` o `custom` | `default` |
| `llm_base_url` | endpoint OpenAI-compatible | local `http://127.0.0.1:8081/v1` |
| `llm_model` | identificador del modelo corrector | `ornith-1.0-9b` |
| `llm_timeout_secs` | timeout de la corrección HTTP | `25` |

La API key no se guarda en texto plano en `config.json`: en Windows se almacena
usando Credential Manager y se reutilizan las configuraciones antiguas durante
la migración.

Los valores inválidos o ausentes se ignoran con el default: la config nunca
rompe el arranque, y el atajo real registrado se muestra en la UI (Ajustes).

## Tests

```powershell
cd E:\AI\FREEBUFF
C:\Users\usuario\.cargo\bin\cargo.exe test   # 160 tests (dominio, config, audio, burbuja, UI)
```

## Estructura

| Fichero | Contenido |
|---|---|
| `src/main.rs` | entrada Iced |
| `src/app.rs` | UI + flujo de dictado |
| `src/domain/*.rs` | lógica pura (limpieza fija, corrección, diccionario) |
| `src/audio.rs` | captura de micrófono (cpal) |
| `src/transcribe.rs` | modelos + whisper.cpp |
| `src/inject.rs` | escritura en app activa |
| `src/hotkey.rs` | atajo global |
| `src/config.rs` | configuración persistente |

## Historial de versiones

- **0.2.0** (rama `native-iced`): reescritura a escritorio nativo. Adiós web.
- 0.1.0 (rama `main`): prototipo web (React + Tauri v2 + Whisper WASM), 40 tests.