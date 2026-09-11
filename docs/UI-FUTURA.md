# UI FUTURA — checklist `app-design-2026`: qué queda por hacer

Base: el skill `app-design-2026` (tendencias 2026 + checklist). El rediseño
(commit `962342c`, `src/theme.rs`) implementó gran parte del lenguaje visual y
la fase R3/R4 (commit `7f2109e`) cerró varios puntos de esta lista. Este
documento lista lo que faltaba y marca el **estado real a 11/09/2026**: lo
hecho queda anotado con su commit, y lo que sigue abierto, como pendiente.

---

## 1. Qué ya cumple (no rehacer)

- **Dark mode por defecto** + toggle persistido (`config.is_dark()`).
- **Tokens centralizados** (`src/theme.rs`): paleta grafito/violeta único
  `#7C6CFF`, grid de 8 px, radios 8–10 px, escala tipográfica. Cero literales
  mágicos en la UI.
- **Calm design / progressive disclosure**: una pantalla, una pregunta (¿qué
  estoy dictando?); ajustes ocultos tras botón, no barra llena.
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
- **PENDIENTE** — **`prefers-reduced-motion`**: Iced lo respeta parcialmente
  vía winit; falta auditar que ningún micro-feedback fuerce animación.
- **HECHO donde el framework lo permite** (`7f2109e`) — **Foco de teclado
  visible**: el `text_input` y el editor dibujan un anillo de acento de 1.5 px
  al enfocar. En botón, checkbox y `pick_list` iced 0.13 no expone estado de
  foco (limitación del framework, anotada en `NOTA-SESION.md`).
- **PENDIENTE** — **Targets táctiles**: el círculo del hero y la burbuja
  (56 px) ya son grandes; los botones secundarios («Copiar», «Limpiar») pueden
  necesitar más área de hit.

### P2 — Estado y feedback fino — PARCIAL

- **HECHO** (R2) — **Errores en idioma del usuario**: los errores de descarga,
  micrófono e inyección son frases cortas y accionables en español
  (`friendly_download_error` y compañía); el detalle técnico queda en el log.
- **HECHO** (R4) — **Estado de descarga del modelo**: además del porcentaje se
  muestra el tamaño (`descargado / total`) y el tiempo restante estimado.
- **PENDIENTE** — **Nivel de micrófono con marcador de pico**: el pico ya se
  calcula y se expone desde `AudioCapture`, pero la UI aún dibuja una barra
  plana; falta el indicador de pico (mic saturado o mudo).

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
`bubble_animation_fps`). Validación en el render: si la ruta
desaparece o cambia a formato no soportado, el getter
correspondiente devuelve `None` y la UI pinta el aspecto original sin
crash.

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
