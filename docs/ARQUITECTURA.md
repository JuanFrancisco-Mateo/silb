# ARQUITECTURA — recorrido del código para quien nunca ha tocado Rust

Este documento explica **qué hace cada fichero** de Whispr Open y **cómo viaja
una frase** desde que hablas hasta que aparece escrita en Slack. No hace falta
saber Rust para leerlo: solo entender qué es un «fichero de código» y qué es
una «función» (una pieza con nombre que recibe datos y devuelve datos).

Si quieres cambiar algo de la app, la última sección dice exactamente qué
fichero tocar.

---

## 1. El mapa en una imagen

```
                         ┌────────────────────────────────────────────────┐
                         │              whispr-open.exe (1 proceso)       │
                         │                                                │
  ┌──────────┐           │  ┌────────────┐    ┌──────────────────────┐    │
  │ TU VOZ   │  micrófono│  │  audio.rs  │──▶│  transcribe.rs       │    │
  └──────────┴───────────┼─▶│  (cpal)    │f32 │  (whisper.cpp+CUDA)  │    │
                         │  └────────────┘16k └──────────┬───────────┘    │
                         │  ┌────────────┐               │ texto crudo    │
  ┌──────────┐ atajo     │  │ hotkey.rs  │               ▼                │
  │ TECLADO  ├───────────┼─▶│Ctrl+Shift+D│    ┌──────────────────────┐    │
  └──────────┘ (global)  │  └────────────┘    │  domain/ (lógica     │    │
                         │  ┌────────────┐    │  pura, sin I/O)      │    │
  ┌──────────┐ bandeja   │  │  tray.rs   │    │  cleanup (muletillas)│    │
  │ RATÓN    ├───────────┼─▶│ (menú)     │    │  dictionary (jerga)  │    │
  └──────────┘           │  └────────────┘    │  correction (reglas) │    │
                         │  ┌────────────┐    │  corrective (decide  │    │
  ┌──────────┐ burbuja   │  │ bubble.rs  │    │   si hace falta IA)  │    │
  │ RATÓN    ├───────────┼─▶│ (2ª ventana)│    └──────────┬───────────┘    │
  └──────────┘           │  └────────────┘               │ texto limpio   │
                         │  ┌────────────┐               ▼                │
                         │  │  llm.rs    │◀── corrección IA opcional      │
                         │  │ (llama.cpp │   (server local en 2º plano)   │
                         │  │  nativo)   │──▶ texto corregido             │
                         │  └────────────┘               │                │
                         │  ┌────────────┐               ▼                │
                         │  │  app.rs    │    ┌──────────────────────┐    │
                         │  │ (ventana + │──▶ │  inject.rs           │    │
                         │  │  nota      │    │ portapapeles + Ctrl+V│    │
                         │  │  editable) │    └──────────┬───────────┘    │
                         │  └────────────┘               │                │
                         │  ┌────────────┐               ▼                │
                         │  │ theme.rs   │    ┌──────────────────────┐    │
                         │  │ (colores   │    │  LA APP CON FOCO     │    │
                         │  │  medidos)  │    │  (Slack, Notion…)    │    │
                         │  └────────────┘    └──────────────────────┘    │
                         │  config.rs (config.json) · single_instance.rs  │
                         │  fonts.rs · log.rs (whispr-open.log)           │
                         └────────────────────────────────────────────────┘
```

Lo importante del dibujo: **todo ocurre dentro de un único proceso**. La app
es un solo `.exe` sin servidor local obligatorio; el único proceso externo
que puede llegar a existir es `llama-server.exe` (la corrección IA), y lo
arranca y apaga la propia app.

---

## 2. Qué es cada fichero (y por qué existe)

### Entrada y configuración

| Fichero | Qué hace | Por qué es así |
|---|---|---|
| `src/main.rs` | El «arranque»: crea la ventana principal y enciende el bucle de la UI. También comprueba que no haya otra instancia abierta. | En Rust todo programa empieza en `main`. El guard de instancia única va aquí para cortar ANTES de registrar atajos ni bandejas duplicadas. |
| `src/config.rs` | Lee y escribe `%APPDATA%\whispr-open\config.json` (modelo elegido, tema, muletillas, proveedor de corrección…). Guarda la API key en el **Administrador de credenciales de Windows**, nunca en el JSON. | La config nunca puede romper el arranque: un fichero corrupto o de una versión vieja cae a valores por defecto, campo por campo. |
| `src/single_instance.rs` | Impide dos copias de la app a la vez con un «mutex» de Windows (un candado con nombre). | Sin esto, la segunda copia intentaría registrar el mismo atajo global y fallaría a ciegas. |
| `src/log.rs` | Escribe el log en `%APPDATA%\whispr-open\whispr-open.log`. | La app no tiene consola (es GUI pura): sin log a archivo, cualquier fallo sería invisible. |

### Entrada del usuario (voz, teclado, ratón)

| Fichero | Qué hace | Por qué es así |
|---|---|---|
| `src/audio.rs` | Abre el micrófono y acumula el sonido (números decimales `f32`, la forma en que el PC guarda el audio) hasta 16 000 muestras por segundo. | El micrófono puede morir a mitad de nota: el módulo avisa por un canal y la app corta el turno con lo grabado. Tope de 2 h para acotar la memoria. |
| `src/hotkey.rs` | El atajo global (por defecto Ctrl+Shift+D): funciona estés en la app que estés. | Un atajo «global» lo registra el sistema operativo; si otra app ya lo usa, la app degrada a modo «solo botones» en vez de romperse. |
| `src/tray.rs` | El icono junto al reloj con su menú (Mostrar/Ocultar, Salir). | Cerrar la ventana no debe cerrar la app: dictado y burbuja siguen vivos en la bandeja. |
| `src/bubble.rs` | La burbuja flotante (círculo violeta que se arrastra con el botón derecho y graba con el izquierdo). Es una **segunda ventana** del mismo proceso. | Dictar sin mirar la ventana principal. No roba el foco (estilo de Windows «no activar») para no molestar a la app donde escribes. |

### El cerebro (todo local)

| Fichero | Qué hace | Por qué es así |
|---|---|---|
| `src/transcribe.rs` | Descarga el modelo de voz (si falta) y transcribe: audio → texto. Usa whisper.cpp con CUDA (GPU). | La descarga se hace a un fichero temporal `.part` y solo al terminar se renombra: un corte de luz jamás deja un modelo a medias. Un modelo truncado se detecta (magic bytes) y se re-descarga solo. |
| `src/domain/cleanup.rs` | Quita muletillas («eh», «o sea»…) y repeticiones («el el» → «el»). | Lógica pura sin I/O: se prueba con tests sin micrófono ni red. |
| `src/domain/dictionary.rs` | Reemplaza jerga: «whatsapp» → «WhatsApp» según tu diccionario. | Igual: pura y testeada. |
| `src/domain/correction.rs` | Correcciones deterministas («no, quería decir X»). | Las reglas son gratis (no gastan IA): se aplican antes de considerar al LLM. |
| `src/domain/corrective.rs` | **Decide** si una frase lleva intención correctiva que las reglas no cubren (y aplica los cambios del LLM). | Un «clasificador» barato evita llamar a la IA en cada turno: solo se paga cuando hace falta. |
| `src/llm.rs` | Habla con el corrector IA: un servidor local llama.cpp que la app lanza y apaga, o un endpoint externo (OpenAI-compatible). | El servidor es un proceso de Windows nativo que la app arranca sola y cierra al salir: nunca queda huérfano en memoria. |

### La salida

| Fichero | Qué hace | Por qué es así |
|---|---|---|
| `src/inject.rs` | Escribe el texto en la app que tiene el foco: lo copia al portapapeles y simula Ctrl+V (o teclea directo, si lo configuras). | Pegar es lo más compatible. Si no hay ninguna ventana con foco, el texto queda en el portapapeles con aviso: nunca se pierde. |
| `src/app.rs` | La ventana principal: la nota **editable**, los botones, los Ajustes, y el «director de orquesta» que conecta todos los módulos anteriores. | Es el único fichero grande porque es el único que toca la UI; la lógica vive en los demás. |
| `src/theme.rs` | Los colores, tamaños y radios de toda la interfaz, con el contraste **medido** (fórmulas WCAG) en tests. | Un solo sitio donde cambiar el aspecto; los tests impiden que un cambio rompa la legibilidad. |
| `src/fonts.rs` | Carga la fuente de emoji de Windows para que símbolos como ✓ y ↩ no salgan como cajas vacías. | El renderizador de texto de la UI no hace «fallback» de fuente por defecto. |

---

## 3. Viaje de una frase: del micrófono a Slack

Pasos exactos que ocurren cuando dices **«la reunión es a las cuatro digo
las cinco»** con Slack enfocado:

```
 1. PULSAS Ctrl+Shift+D ──── Windows avisa a hotkey.rs (canal de eventos)
 2. app.rs recibe el aviso ── arranca audio.rs: el micrófono empieza a
                              acumular sonido (f32, 16 kHz)
 3. HABLAS ────────────────── el medidor de nivel de la UI se mueve con tu
                              voz (feedback de que te oye)
 4. VUELVES A PULSAR ──────── audio.rs entrega el audio a transcribe.rs
                              EN UN HILO APARTE (la UI nunca se congela)
 5. WHISPER (GPU) ─────────── audio → «la reunión es a las cuatro digo las
                              cinco» (texto crudo, 100 % en tu PC)
 6. LIMPIEZA (domain/) ────── muletillas fuera; diccionario aplicado
 7. ¿CORRECCIÓN? (corrective) la frase trae «digo»: hay auto-corrección
                              hablada → se envía al corrector IA
 8. LLM (llm.rs) ──────────── en otro hilo: «la reunión es a las cinco»
                              (si no hubiera IA disponible, se insertaría
                              el texto sin corregir: nunca se bloquea)
 9. NOTA (app.rs) ─────────── el texto aparece en la ventana, EDITABLE:
                              puedes retocarlo a mano antes de mandarlo
10. INYECCIÓN (inject.rs) ─── se copia al portapapeles y se simula Ctrl+V:
                              «la reunión es a las cinco» aparece en Slack
11. CONFIRMACIÓN ──────────── la app te enseña qué corrigió («cuatro →
                              cinco») y un «↩ Deshacer» por si no era eso
```

Detalles que no se ven en el dibujo pero importan:

- **Hilos**: la transcripción (4-5) y la corrección IA (8) ocurren en hilos
  aparte para que la ventana nunca se congele. Se comunican con la UI por
  «canales» (colas de mensajes) que la UI drena 10 veces por segundo.
- **Nunca se pierde texto**: si la IA falla, se agota su tiempo o su
  respuesta no convence a los filtros de seguridad, se escribe EXACTAMENTE
  lo que dijiste, sin corrección.
- **Todo local salvo el paso 8**: la transcripción (whisper) siempre ocurre
  en tu PC. La corrección IA es local por defecto; si la configuras en la
  nube, el pie de la ventana te lo recuerda.

---

## 4. Qué tocar para cambiar cada cosa

| Quiero cambiar… | Fichero | Dónde exactamente |
|---|---|---|
| El atajo por defecto | `src/hotkey.rs` | `DEFAULT_SHORTCUT` (y su test) |
| Las muletillas que se limpian | `src/domain/cleanup.rs` | `default_unambiguous` / `default_ambiguous` / `default_start_of_turn` (o desde Ajustes, sin tocar código) |
| Los colores de la UI | `src/theme.rs` | los `const` de arriba; los tests de contraste te avisan si el nuevo color no se lee bien |
| El modelo de voz por defecto | `src/config.rs` | `selected_model()` |
| Las frases que activan la corrección IA | `src/domain/corrective.rs` | los patrones de `classify_in_turn` / `classify` |
| El comportamiento del corrector IA | `src/llm.rs` | `SYSTEM_PROMPT` / `SYSTEM_PROMPT_IN_TURN` y el guard anti-truncación |
| Dónde se instalan los modelos | `src/transcribe.rs` | `models_dir()` |
| Cómo se escribe en otras apps | `src/inject.rs` | `inject_text_into_active_app` (paste/type) |
| Textos de la interfaz | `src/app.rs` | directamente en la vista (están en español y sin constantes: son fáciles de encontrar con Ctrl+F) |
| El tiempo máximo de una nota | `src/audio.rs` | `MAX_RECORDING_SECS` |

Regla de oro del repo: **la lógica va en `src/domain/` (pura, sin I/O,
testeable) y la UI en `src/app.rs`**. Si añades una feature, primero la
función pura con sus tests, luego el cableado en `app.rs`.

---

## 5. Cómo se prueba todo esto sin micrófono

Cada módulo trae sus tests («unit tests»: funciones que comprueban otras
funciones con ejemplos conocidos):

```powershell
cd E:\AI\FREEBUFF
C:\Users\usuario\.cargo\bin\cargo.exe test    # 271 pasan, 1 se salta
```

El que se salta («ignored») necesita un servidor llama.cpp real encendido:
es una prueba de integración viva, no un fallo.

Lo que los tests **no** pueden cubrir y se prueba a mano: el micrófono
físico, el foco real entre ventanas y la inyección en apps elevadas
(administrador). La checklist está en `INFORME-CIERRE.md`.

## 6. Cosas que NO hace esta app (a propósito)

- Sin telemetría: no hay ninguna llamada a servicios externos salvo las que
  el usuario configura (corrección IA en la nube) y la descarga del modelo de
  voz en el primer arranque (HuggingFace, solamente si falta el modelo).
- Sin frontend web: no hay HTML/CSS/JS ni webview; la UI es nativa (Iced).
- Sin auto-actualización: la app no se descarga a sí misma.
- El instalador no mete los modelos de voz dentro (se bajan al primer
  arranque, una vez).
