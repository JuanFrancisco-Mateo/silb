# UI FUTURA — mejoras pendientes según `app-design-2026`

Base: el skill `app-design-2026` (tendencias 2026 + checklist). El rediseño ya
hecho (commit `962342c`, `src/theme.rs`) implementa gran parte del lenguaje
visual. Este documento lista **lo que falta** para cerrar el checklist, en
orden de impacto.

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

## 2. Mejoras pendientes (priorizadas)

### P0 — AI transparente y overrideable (el más importante)

El principio nº3 del skill: si la app usa IA, el usuario debe **ver** qué hace
y poder **corregirla/anularla**.

- **Trust signal persistente**: un texto discreto y siempre visible del tipo
  «Transcripción local · nada sale de tu PC» en el footer. Hoy esa promesa
  solo está en el README, no en la app.
- **Overflow del transcript editable**: hoy `transcript` es `text` de solo
  lectura. Permitir **editar el texto** directamente (clic y corregir), para
  que un error de Whisper no dependa de re-dictar ni de una corrección por voz.
  Iced: sustituir el `text` del transcript por un `text_input` multilínea (o
  un editor) con `Message::TranscriptEdited`.
- **Confianza mostrada, no magia**: cuando se aplique la corrección por voz
  (`CORRECCION-VOZ.md`), mostrar el diff («4 → 2») y ofrecer **Deshacer**.
  Este punto se implementa con esa etapa; no duplicar aquí.

### P1 — Contraste y accesibilidad (WCAG como infraestructura)

- **Contraste del acento**: el violeta `#7C6CFF` sobre fondo grafito
  `#17181C` roza el límite para texto pequeño. Testear ≥ 4.5:1 en texto y ≥
  3:1 en elementos grandes; si falla, subir la luminancia del acento (p. ej.
  `#8F82FF`, ya usado como hover) o reservar el acento para fondos y usar
  `#F7F6F3` para texto.
- **`prefers-reduced-motion`**: Iced lo respeta parcialmente vía winit; auditar
  que ningún micro-feedback (transiciones del hero, progress) fuerce animación.
- **Foco de teclado visible**: verificar que todos los controles (toggle de
  tema, ajustes, checkbox, pick_list) tienen focus ring visible; hoy depende
  de los estilos por defecto de Iced.
- **Targets táctiles**: el círculo del hero y la burbuja (56 px) ya son
  grandes; los botones secundarios («Copiar», «Limpiar») pueden necesitar más
  área de hit.

### P2 — Estado y feedback fino

- **Errores en idioma del usuario**: `Status::Error` ya muestra el `hint`
  crudo; revisar que los mensajes de error (descarga, micrófono, inyección)
  sean frases amables y accionables, no mensajes técnicos de la cadena de
  crates.
- **Estado de descarga del modelo** más claro: hoy usa el hero con barra; añadir
  el tiempo restante o el tamaño (`descargado / total`) junto al porcentaje.
- **Nivel de micrófono con marcador de pico**: el `progress_bar` actual es
  plano; un indicador de pico ayuda a saber si el mic está saturado o mudo.

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
