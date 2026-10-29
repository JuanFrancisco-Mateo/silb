# UI FUTURA — checklist `app-design-2026`: qué queda por hacer

Base: el skill `app-design-2026` (tendencias 2026 + lista de comprobación). El
rediseño (commit `962342c`, `src/theme.rs`) puso en marcha buena parte del
lenguaje visual y la fase R3/R4 (commit `7f2109e`) cerró varios puntos de esta
lista. Este documento cuenta lo que faltaba y marca el **estado real a
11/09/2026**, con la revisión del **24/09/2026** (`da2a622` log, `2ef87bb`
assets, `e5fc6ed` secciones de la ventana principal): lo hecho queda anotado con
su commit, y lo que sigue abierto, como pendiente.

---

## 1. Qué ya cumple (no rehacer)

- **Modo oscuro por defecto** + interruptor que se recuerda
  (`config.is_dark()`).
- **Medidas centralizadas** (`src/theme.rs`): paleta grafito/violeta único
  `#7C6CFF`, rejilla de 8 px, radios 8–10 px, escala de tamaños de letra. Cero
  valores sueltos en la interfaz.
- **Diseño tranquilo / mostrar solo lo necesario**: una pantalla, una pregunta
  (¿qué estoy dictando?) y **dos secciones** en la ventana principal (Dictado /
  Ajustes) con la MISMA barra de pestañas que el panel; nada de barras llenas ni
  pestañas de adorno (`e5fc6ed`).
- **Dos señales del estado del micro** (color + punto + etiqueta), medidor de
  nivel, sin animaciones de adorno, sin cristal traslúcido ni relieves falsos.
- **Estado principal a la vista**: el botón grande del micrófono es el elemento
  central.

---

## 2. Mejoras priorizadas (con estado real a 11/09/2026)

### P0 — AI transparente y overrideable (el más importante) — HECHO

El principio nº3 del skill: si la app usa IA, quien la usa debe **ver** qué hace
y poder **corregirla o anularla**.

- **HECHO** (11/09/2026) — **Aviso de confianza siempre a la vista**: «Transcripción
  local · nada sale de tu PC» en el pie de la ventana principal (`src/app.rs`),
  con el texto cambiado cuando toca: si el proveedor de corrección es la nube, NO
  se promete que nada sale del PC (hay un test que lo comprueba).
- **HECHO** (`7f2109e`) — **Texto editable**: lo transcrito es un `text_editor`
  multilínea (`Message::TranscriptEdited`); editar a mano y seguir dictando no se
  pisan (lo dictado se añade al final del texto editado).
- **HECHO** (`CORRECCION-VOZ.md`) — **Enseñar lo que se cambió, no magia**: la
  corrección por voz muestra el diff («4 → 2») y ofrece Deshacer.

### P1 — Contraste y accesibilidad (WCAG como infraestructura) — PARCIAL

- **HECHO** (`7f2109e`) — **Contraste del acento**: `contrast_ratio` (WCAG)
  calculado en el código y tests que exigen ≥ 4.5:1 en texto y ≥ 3:1 en gráficos;
  el acento de texto (`ACCENT_TEXT`) es más claro que el de marca para cumplirlo.
- **REVISADO** (`e5fc6ed`) — **`prefers-reduced-motion`**: no hay ninguna
  animación de adorno que revisar. Las dos únicas actualizaciones periódicas son
  el latido de estado de 100 ms (vacía canales, no dibuja movimiento) y la
  animación por frames de la burbuja, que es material del PROPIO usuario y solo se
  programa si tiene frames configurados (sin frames, `Subscription::none()`). No
  queda ningún efecto forzado; si algún día se añade un fundido, debe mirar antes
  la preferencia del sistema (winit).
- **HECHO donde el framework lo permite** (`7f2109e`) — **Foco de teclado
  visible**: el `text_input` y el editor dibujan un anillo de acento de 1.5 px al
  enfocar. En botón, checkbox y `pick_list`, iced 0.13 no expone el estado de
  foco (limitación del framework, anotada en `NOTA-SESION.md`).
- **HECHO** (`e5fc6ed`) — **Zonas para pulsar**: el círculo grande y la burbuja
  (56 px) ya eran grandes; los botones secundarios («Copiar», «Limpiar»,
  «Inyectar», «Deshacer», «Descartar») pasan por `secondary_button()` con el
  margen para dedo del tema (`SECONDARY_BUTTON_PADDING`, pasos de la rejilla de
  8 px) en vez del margen por defecto de iced.

### P2 — Estado y feedback fino — PARCIAL

- **HECHO** (R2) — **Errores en el idioma de quien usa la app**: los errores de
  descarga, micrófono y escritura son frases cortas y accionables en español
  (`friendly_download_error` y compañía); el detalle técnico queda en el
  registro.
- **HECHO** (R4) — **Estado de la descarga del modelo**: además del porcentaje se
  ve el tamaño (`descargado / total`) y el tiempo que queda estimado.
- **HECHO** (`e5fc6ed`) — **Nivel de micrófono con marca de pico**: la barra
  dibuja el PICO que guarda `AudioCapture`, así que se etiqueta (`pico N%`) y
  avisa cuando hace falta: «sin señal del micrófono: ¿está silenciado?» pasado un
  margen de turno (`MIC_SILENT_SECS`) y «pico al máximo: el micrófono puede estar
  saturando» al saturar. Función pura `mic_peak_hint` + colores del tema (claro y
  oscuro) y texto, no solo color.

### P3 — Personalidad (opcional, bajo coste)

- **Mascota de marca** (tendencia 2026): el proyecto ya usa «Paul 🐶» como
  referencia; un pequeño estado de mascota (dormida/listo/grabando) en la burbuja
  o en el botón grande del micro da personalidad sin añadir complicación.
  Opcional y se puede quitar.
- **Pantalla vacía del texto** con una ayuda visual pequeña (además del texto de
  ejemplo que hay ahora).

### P4 — Interacción multimodal

- **Voz + teclado + gesto** ya conviven (atajo global, botón, burbuja). Candidato
  futuro: **gesto de ratón** en la burbuja (doble clic = repetir el último turno),
  sin tocar la detección de arrastre actual (>7 px).

---

## 7. Personalización III — imágenes y animación propia (cierre)

iced 0.13 no trae un widget de imagen animada (no hay soporte nativo de
GIF/WebP), pero quien usa la app puede poner su propio material visual:

- **Imagen de fondo de la ventana principal** (PNG/JPG/BMP/GIF/WebP/TIFF). Se
  dibuja con `iced::widget::image` apilada debajo del contenido
  (`iced::widget::stack!`) y `ContentFit::Cover`. Si la ruta no existe o el
  formato no se puede leer, la app vuelve al fondo del tema (nunca una ventana
  negra).
- **Sprite fijo de la burbuja** (los mismos formatos). Si falla la carga, vuelve
  igual al dibujo normal.
- **Animación por frames**: una carpeta con PNGs (u otro formato admitido) que se
  van cambiando a un FPS configurable (1–30, por defecto 12) con
  `iced::time::every`. Orden alfabético; al llegar al final vuelve a empezar sin
  fallos. Si la carpeta está vacía o no tiene imágenes, pasa al sprite fijo o al
  dibujo normal.

Las rutas se guardan como dato en `config.json` (`bg_image_path`,
`bubble_image_path`, `bubble_frames_path`, `bubble_animation_fps`). Desde el
24/09/2026 (`2ef87bb`, H4) lo que se guarda NO es el fichero que eligió la
persona, sino una **copia** en `%APPDATA%\whispr-open\assets\`: si vacía
Descargas, la personalización sigue ahí (antes desaparecía sin avisar). Una
configuración antigua con una ruta externa que siga existiendo se respeta tal
cual; solo al arrancar se quitan del config las rutas que ya no llevan a nada.
Comprobación al dibujar: si la función que la resuelve no encuentra nada, la
interfaz pinta el aspecto original sin romperse y queda «asset desaparecido,
cayendo al tema» en el registro.

## 8. Secciones de la VENTANA PRINCIPAL (HECHO, `e5fc6ed`)

«Ajustes ya tiene pestañas» pasó a ser «la app entera tiene pestañas». La ventana
principal reorganiza lo que ya existía en dos secciones — **Dictado** (aviso del
corrector, botón grande del micro, nota, acciones del turno) y **Ajustes** (el
panel completo) — con una barra de pestañas compartida (`src/ui/tabs.rs`,
`tab_bar`), que entonces tenían Ajustes y la ventana:

- Una sola forma de hacer pestañas: la activa con `●` + fondo + borde de acento;
  el texto siempre a la vista (no se distingue solo por color).
- No hay estado nuevo: la sección es una LECTURA de `settings_open`
  (`WhisprApp::window_section`), así que los manejadores de Ajustes y lo que se
  guarda no cambian. Abrir «Ajustes» abre el panel; volver a «Dictado» lo cierra
  y deja la pestaña General.
- La cabecera se queda solo con el botón de tema; el botón «Ajustes ▸»
  desaparece.
- Todo color, espacio y radio sale de `src/theme.rs` (cero valores nuevos
  sueltos).

> Nota (fase 2 del rediseño, frame Ajustes): `src/ui/tabs.rs` se retiró. La
> navegación entre secciones es la barra lateral del marco de Figma y la
> navegación de Ajustes es la suya (`ui::settings::settings_nav`, subrayado de
> acento en la pestaña activa).

## 9. Registro con rotación (H2, `da2a622`)

El registro había llegado a 1,14 MB / 11.102 líneas, de las que el 79% (8.761)
eran una línea por cada evento de ratón al arrastrar la burbuja. Ahora:

- **Rotación por tamaño** en `src/log.rs` (con la biblioteca estándar, sin
  dependencias): al pasar de ~1 MB el fichero pasa a `.1` y el `.1` a `.2` (dos
  históricos), y se sigue escribiendo en el nuevo.
- Las trazas de mucha frecuencia (`drag start` / `drag target`) son de nivel
  **debug**: solo se escriben con `WHISPR_DEBUG=1`. Por defecto el registro no se
  llena de ruido.
- El arranque apunta la **ruta del registro**, y **Ajustes → Avanzado** la enseña
  con un botón «Abrir carpeta del registro» (además del estado de
  `WHISPR_DEBUG` y de dónde viven las imágenes propias).

## 3. Cómo aplicar (patrones Iced, del propio skill)

- Todo color, espacio y radio sale de `src/theme.rs`: si un hueco pide una medida
  nueva (por ejemplo, un color de «confianza» para el aviso), se añade ahí y se
  usa desde ahí, nunca un valor suelto en `app.rs`.
- Oscuro por defecto: ya resuelto; el aviso de confianza y el texto editable deben
  usar `tokens(theme)` para respetar el modo.
- Sin animaciones de entrada porque sí; solo respuestas cortas de estado
  (150–250 ms), y respetando la preferencia de menos movimiento.
- Mostrar solo lo necesario: las mejoras P2 (tiempo restante, pico) se añaden al
  panel de ajustes o al botón grande sin crear una barra de herramientas llena.

---

## 4. Criterios de aceptación (por ítem)

- Aviso de confianza visible en los dos modos, con contraste AA.
- Texto editable: se puede corregir a mano y el cambio se mantiene en
  `transcript`/`last_turn` (sin romper el camino hacia la escritura en otras
  apps).
- Acento comprobado a ≥ 4.5:1 (o cambiado) para texto sobre oscuro.
- Foco de teclado visible en todos los controles.
- `cargo test` / `cargo check` verdes; medidas centralizadas sin valores nuevos
  sueltos.

---

## VISIÓN DEL USUARIO 2026-10-29 — elegir modelo de Hugging Face desde Ajustes — **NO implementar todavía (0.7)**

Guía de diseño para la 0.7. Son sus palabras, tal cual las dijo, con el
contexto de la pantalla que estaba mirando (Ajustes → Corrección IA):

1. En **Ajustes → Corrección IA**, el campo de texto «Modelo IA local» (hoy
   escribes a mano `qwen2.5-0.5b-instruct-q4_k_m.gguf`) tiene que ser un
   **desplegable**, o abrir un **menú**, para elegir un modelo de Hugging Face.
2. Al elegir uno, **la app lo descarga** (como ya hace con el modelo de
   whisper). Nada de ir a Hugging Face a mano, bajar el GGUF y dejarlo en
   `models-llm`.
3. Es para la **0.7**, no para la 0.6.

**PREGUNTA ABIERTA** — no decidir por cuenta propia:

- ¿Desplegable con una **lista corta elegida por nosotros** (los ya
  recomendados en esa misma pantalla: Qwen2.5-0.5B, Llama 3.2 1B, Qwen2.5-1.5B)
  o un **buscador libre** de Hugging Face donde pegas el repo o la URL?
- Si alguien elige un modelo que **no es GGUF** (safetensors, por ejemplo),
  ¿se avisa y se descarta, o se intenta convertir? (whisper ya trabaja solo con
  GGUF, y el servidor LLM embarcado también.)

Encaja en el sistema de **Corrección IA** que ya existe (`docs/CORRECCION-VOZ.md`):
hoy el modelo local se resuelve por nombre en `models-llm`; esto añade el paso
«elegirlo y traerlo» delante, sin tocar el resto del camino.
