# UI FUTURA — checklist `app-design-2026`: qué queda por hacer

Base: el skill `app-design-2026` (tendencias 2026 + checklist). El rediseño
(commit `962342c`, `src/theme.rs`) implementó gran parte del lenguaje visual y
la fase R3/R4 (commit `7f2109e`) cerró varios puntos de esta lista. Este
documento lista lo que faltaba y marca el **estado real a 11/09/2026**, con la
revisión del **24/09/2026** (`da2a622` log, `2ef87bb` assets, `e5fc6ed`
secciones de la ventana principal): lo hecho queda anotado con su commit, y lo
que sigue abierto, como pendiente.

---

## 1. Qué ya cumple (no rehacer)

- **Dark mode por defecto** + toggle persistido (`config.is_dark()`).
- **Tokens centralizados** (`src/theme.rs`): paleta grafito/violeta único
  `#7C6CFF`, grid de 8 px, radios 8–10 px, escala tipográfica. Cero literales
  mágicos en la UI.
- **Calm design / progressive disclosure**: una pantalla, una pregunta (¿qué
  estoy dictando?) y **dos secciones** en la ventana principal (Dictado /
  Ajustes) con la MISMA barra de pestañas que el panel; nada de barras
  densas ni de pestañas decorativas (`e5fc6ed`).
- **Feedback doble** del estado del micro (color + punto + label), medidor de
  nivel, sin animaciones decorativas, sin glassmorphism/neumorphism.
- **Estado primario visible**: el hero del micrófono es el elemento central.

---

## 2. Mejoras priorizadas (con estado real a 11/09/2026)

### P0 — AI transparente y overrideable (el más importante) — HECHO

El principio nº3 del skill: si la app usa IA, el usuario debe **ver** qué hace
y poder **corregirla/anularla**.

- **HECHO** (11/09/2026) — **Trust signal persistente**: «Transcripción local ·
  nada sale de tu PC» en el pie de la ventana principal (`src/app.rs`), con el
  texto adaptado: si el proveedor de corrección es la nube, NO se promete que
  nada sale del PC (hay un test que lo comprueba).
- **HECHO** (`7f2109e`) — **Transcript editable**: el transcript es un
  `text_editor` multilínea (`Message::TranscriptEdited`); editar a mano y
  seguir dictando no se pisan (el dictado se añade al final del texto editado).
- **HECHO** (`CORRECCION-VOZ.md`) — **Confianza mostrada, no magia**: la
  corrección por voz muestra el diff («4 → 2») y ofrece Deshacer.

### P1 — Contraste y accesibilidad (WCAG como infraestructura) — PARCIAL

- **HECHO** (`7f2109e`) — **Contraste del acento**: `contrast_ratio` (WCAG)
  calculado en código y tests que exigen ≥ 4.5:1 en texto y ≥ 3:1 en
  gráficos; el acento de texto (`ACCENT_TEXT`) es más claro que el de marca
  para cumplirlo.
- **AUDITADO** (`e5fc6ed`) — **`prefers-reduced-motion`**: no hay ninguna
  animación decorativa que auditar. Las dos únicas suscripciones temporales
  son el latido de estado de 100 ms (drena canales, no pinta movimiento) y la
  animación por frames de la burbuja, que es material del PROPIO usuario y
  solo se agenda si tiene frames configurados (sin frames,
  `Subscription::none()`). No queda micro-feedback forzado; si algún día se
  añade un fundido, debe mirar antes la preferencia del sistema (winit).
- **HECHO donde el framework lo permite** (`7f2109e`) — **Foco de teclado
  visible**: el `text_input` y el editor dibujan un anillo de acento de 1.5 px
  al enfocar. En botón, checkbox y `pick_list` iced 0.13 no expone estado de
  foco (limitación del framework, anotada en `NOTA-SESION.md`).
- **HECHO** (`e5fc6ed`) — **Targets táctiles**: el círculo del hero y la
  burbuja (56 px) ya eran grandes; los botones secundarios («Copiar»,
  «Limpiar», «Inyectar», «Deshacer», «Descartar») pasan por
  `secondary_button()` con el padding táctil del tema
  (`SECONDARY_BUTTON_PADDING`, pasos del grid de 8 px) en vez del padding por
  defecto de iced.

### P2 — Estado y feedback fino — PARCIAL

- **HECHO** (R2) — **Errores en idioma del usuario**: los errores de descarga,
  micrófono e inyección son frases cortas y accionables en español
  (`friendly_download_error` y compañía); el detalle técnico queda en el log.
- **HECHO** (R4) — **Estado de descarga del modelo**: además del porcentaje se
  muestra el tamaño (`descargado / total`) y el tiempo restante estimado.
- **HECHO** (`e5fc6ed`) — **Nivel de micrófono con marcador de pico**: la
  barra dibuja el PICO que retiene `AudioCapture`, así que se etiqueta
  (`pico N%`) y avisa cuando aporta algo: «sin señal del micrófono: ¿está
  silenciado?» pasado un margen de turno (`MIC_SILENT_SECS`) y «pico al
  máximo: el micrófono puede estar saturando» al saturar. Función pura
  `mic_peak_hint` + colores del tema (claro y oscuro) y texto, no solo color.

### P3 — Personalidad (opcional, bajo coste)

- **Mascota de marca** (tendencia 2026): el proyecto ya usa «Paul 🐶» como
  referencia; un pequeño estado de mascota (dormida/listo/grabando) en la
  burbuja o el hero aporta personalidad sin añadir complejidad. Opcional y
  reversible.
- **Empty-state del transcript** con micro-ayuda visual (además del texto de
  placeholder actual).

### P4 — Interacción multimodal

- **Voz + teclado + gesto** ya coexisten (atajo global, botón, burbuja).
  Candidato futuro: **gesto de ratón** en la burbuja (doble clic = repetir
  último turno), sin tocar la detección de arrastre actual (>7 px).

---

## 7. Personalización III — imágenes y animación propia (cierre)

iced 0.13 no expone un widget de imagen animada (no hay soporte nativo
de GIF/WebP), pero el usuario puede poner su propio material visual:

- **Imagen de fondo de la ventana principal** (PNG/JPG/BMP/GIF/WebP/TIFF).
  Renderizada con `iced::widget::image` apilada bajo el contenido
  (`iced::widget::stack!`) y `ContentFit::Cover`. Si la ruta no existe
  o el formato no es legible, la app cae al fondo del tema (nunca
  ventana negra).
- **Sprite estático de la burbuja flotante** (mismo formato). Mismo
  patrón de fallback al render vectorial si falla la carga.
- **Animación por frames**: carpeta con PNGs (u otro formato
  soportado) que se conmutan a un FPS configurable (1–30, default 12)
  con `iced::time::every`. Orden alfabético; wrap seguro. La carpeta
  vacía o sin imágenes cae al sprite estático o al render vectorial.

Las rutas se persisten como dato en `config.json`
(`bg_image_path`, `bubble_image_path`, `bubble_frames_path`,
`bubble_animation_fps`). Desde el 24/09/2026 (`2ef87bb`, H4) lo que se
guarda NO es el fichero que eligió el usuario, sino una **copia** en
`%APPDATA%\whispr-open\assets\`: si vacía Descargas, la personalización
sigue ahí (antes desaparecía en silencio). Una config vieja con ruta externa
que siga existiendo se respeta tal cual; solo al arrancar se quitan del
config las rutas que ya no apuntan a nada. Validación en el render: si el
getter correspondiente no resuelve, la UI pinta el aspecto original sin crash
y queda «asset desaparecido, cayendo al tema» en el log.

## 8. Secciones de la VENTANA PRINCIPAL (HECHO, `e5fc6ed`)

«Ajustes ya tiene pestañas» pasó a ser «la app entera tiene pestañas». La
ventana principal reorganiza lo que ya existía en dos secciones — **Dictado**
(chip del corrector, hero del micro, nota, acciones del turno) y **Ajustes**
(el panel completo) — con una barra de pestañas compartida (`src/ui/tabs.rs`,
`tab_bar`), que entonces tenían Ajustes y la ventana:

- Una sola implementación de pestañas: la activa con `●` + fondo + borde de
  acento; el texto siempre visible (no se identifica solo por color).
- No hay estado nuevo: la sección es una LECTURA de `settings_open`
  (`WhisprApp::window_section`), así que los handlers de Ajustes y su
  persistencia no cambian. Abrir «Ajustes» abre el panel; volver a «Dictado»
  lo cierra y deja la pestaña General.
- La cabecera se queda solo con el conmutador de tema; el botón «Ajustes ▸»
  desaparece.
- Todo color/espacio/radio sale de `src/theme.rs` (cero literales nuevos).

> Nota (fase 2 del rediseno, frame Ajustes): `src/ui/tabs.rs` se retiró. La
> navegación entre secciones es la barra lateral del marco de Figma y la
> navegación de Ajustes es la suya (`ui::settings::settings_nav`, subrayado de
> acento en la pestaña activa).

## 9. Registro con rotación (H2, `da2a622`)

El log había llegado a 1,14 MB / 11.102 líneas, de las que el 79% (8.761) eran
una línea por evento de ratón del arrastre de la burbuja. Ahora:

- **Rotación por tamaño** en `src/log.rs` (std, sin dependencias): al pasar de
  ~1 MB el fichero pasa a `.1` y el `.1` a `.2` (dos históricos), y se sigue
  escribiendo en el nuevo.
- Las trazas de alta frecuencia (`drag start` / `drag target`) son de nivel
  **debug**: solo se escriben con `WHISPR_DEBUG=1`. Por defecto el log no se
  ensucia.
- El arranque registra la **ruta del log**, y **Ajustes → Avanzado** la
  muestra con un botón «Abrir carpeta del registro» (además del estado de
  `WHISPR_DEBUG` y de dónde viven las imágenes propias).

## 3. Cómo aplicar (patrones Iced, del propio skill)

- Todo color/espacio/radio sale de `src/theme.rs`: si un hueco exige un token
  nuevo (p. ej. un color de "confianza" para el trust signal), añadirlo ahí y
  referenciarlo, nunca un literal en `app.rs`.
- Dark default: ya resuelto; el trust signal y el transcript editable deben
  usar `tokens(theme)` para respetar el modo.
- Sin animaciones de entrada gratuitas; solo micro-feedback de estado
  (150–250 ms), y respetando reduced-motion.
- Progressive disclosure: las mejoras P2 (tiempo restante, pico) se añaden al
  panel de ajustes o al hero sin crear una barra de herramientas densa.

---

## 4. Criterios de aceptación (por ítem)

- Trust signal visible en ambos modos, con contraste AA.
- Transcript editable: el usuario puede corregir el texto a mano y el cambio
  persiste en `transcript`/`last_turn` (sin romper el pipeline de inyección).
- Acento verificado a ≥ 4.5:1 (o cambiado) para texto sobre dark.
- Foco de teclado visible en todos los controles.
- `cargo test` / `cargo check` verdes; tokens centralizados sin literales
  mágicos nuevos.
