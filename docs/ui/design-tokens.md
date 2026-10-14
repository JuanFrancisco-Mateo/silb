# Design tokens — Whispr Open (rediseño de UI)

Fuente de verdad para llevar el diseño de Penpot al código Rust/Iced.
Estado: **Fase 0 completada** (medidas reales del código). Las tablas finales
por tema (Dark / Light / Custom) se completan tras las fases de diseño en
Penpot (archivo *Open Whisper*, `http://localhost:9001`).

---

## 1. Medidas reales del código actual (no inventadas)

| Concepto | Valor | Origen |
| --- | --- | --- |
| Ventana principal | 480 × 800 px | `src/main.rs` (`main_window.size`) |
| Ventana mínima | 380 × 600 px | `src/main.rs` (`min_size`) |
| Burbuja flotante (default) | 56 px (círculo) | `src/bubble.rs` (`BUBBLE_SIZE`) |
| Burbuja flotante (rango) | 48 / 56 / 72 px | `src/config.rs` (`BubbleSize`: Small/Mid/Large) |
| Margen de burbuja al borde | 24 px | `src/bubble.rs` (`BUBBLE_MARGIN`) |
| Offset superior de burbuja | 48 px | `src/bubble.rs` (`BUBBLE_TOP_OFFSET`) |
| Opacidad de burbuja | 20–100 % (default 100) | `src/config.rs` (`bubble_opacity`) |
| Estilos de burbuja | Solid / Ring / Minimal | `src/config.rs` (`BubbleStyle`) |
| Tipografía base | 11 / 12 / 14 / 16 / 20 / 24 px | `src/theme.rs` (`FONT_XS..FONT_HERO`) |
| Factores de tamaño de fuente | 0.85 / 1.0 / 1.15 / 1.3 (clamp 8–64 px) | `src/config.rs` (`FontSize`) |
| Espaciado (grid de 8 px) | 4 / 8 / 16 / 24 / 32 / 48 px | `src/theme.rs` (`SPACE_1..SPACE_6`) |
| Radios | 8 px (sm) / 10 px (md) | `src/theme.rs` (`RADIUS_SM/MD`) |
| Padding botón secundario | 8 × 16 px | `src/theme.rs` (`SECONDARY_BUTTON_PADDING`) |
| Borde de inputs/tarjetas | 1 px | `src/theme.rs` (estilos de widget) |
| Anillo de foco | 1.5 px de acento | `src/theme.rs` (`text_input`, `text_editor`) |
| Duración máxima de grabación | 2 h | `src/audio.rs` (`MAX_RECORDING_SECS`) |
| Animación de burbuja (frames) | 1–30 FPS (default 12) | `src/config.rs` (`bubble_animation_fps`) |

Principios vigentes del lenguaje visual (`src/theme.rs`): calm design, dark
por defecto, un solo acento violeta `#7C6CFF`, sin glassmorphism, sin
animaciones decorativas, grid de 8 px, foco visible, contraste WCAG medido
por tests (texto ≥ 4.5:1, gráficos ≥ 3:1).

---

## 2. Paleta actual por modo (`src/theme.rs`, verificada por tests WCAG)

### Theme/Dark (por defecto)

| Token | Hex | Uso |
| --- | --- | --- |
| `bg` | `#17181C` | Fondo de ventana (grafito) |
| `surface` | `#1F2026` | Tarjetas e inputs |
| `surface-alt` | `#272830` | Hover, fondos secundarios |
| `text` | `#F7F6F3` | Texto principal |
| `dim` | `#9A9AA3` | Texto atenuado (labels, pistas) |
| `accent` | `#7C6CFF` | Marca: rellenos, foco, checks (≥ 3:1) |
| `accent-hover` | `#8F82FF` | Hover del primario |
| `accent-text` | `#8F82FF` | Acento como texto (≥ 4.5:1) |
| `ok` | `#3FB95F` | Estados correctos |
| `warn` | `#F5A623` | Avisos |
| `err` | `#EB5757` | Errores |
| `border` | `#2E3038` | Bordes y separadores |

### Theme/Light

| Token | Hex | Uso |
| --- | --- | --- |
| `bg` | `#F6F5F2` | Fondo de ventana (blanco roto) |
| `surface` | `#FFFFFF` | Tarjetas e inputs |
| `surface-alt` | `#EBEBEF` | Hover, fondos secundarios |
| `text` | `#17181C` | Texto principal |
| `dim` | `#6B6B76` | Texto atenuado |
| `accent` | `#7C6CFF` | Marca (mismo violeta en ambos modos) |
| `accent-hover` | `#9A8CFF` | Hover del primario (más claro en light) |
| `accent-text` | `#6652F2` | Acento como texto (≥ 4.5:1) |
| `ok` | `#1F7A3E` | Estados correctos |
| `warn` | `#8F6300` | Avisos |
| `err` | `#C93E3E` | Errores |
| `border` | `#D9D9E0` | Bordes y separadores |

Tinta fija sobre relleno de acento (botón primario): `#17181C`
(`INK_ON_ACCENT`, ≥ 4.5:1 sobre `#7C6CFF` y su hover en ambos modos).

---

## 3. Tokens por tema (creados en Penpot, proyecto *Open Whisper*)

Sets del catálogo de tokens del archivo (junto al set previo
`Whispr Open · Núcleo`) y themes `Theme/Dark`, `Theme/Light`, `Theme/Custom`
(cada theme activa Núcleo + su set). **Estrategia de nombres**: los tokens de
color llevan prefijo `dark.` / `light.` porque Penpot resuelve los tokens por
NOMBRE con precedencia del último set activo — sin prefijo, los tres temas no
podrían convivir en boards contiguos. Los tokens de `Theme/Custom` llevan los
nombres exactos del encargo (no colisionan).

### Theme/Dark (22 tokens)

| Token | Valor | Uso |
| --- | --- | --- |
| `dark.surface.app` | `#111218` | Fondo de ventana |
| `dark.surface.bubble` | `#15161C` | Superficie de burbuja |
| `dark.surface.panel` | `#181920` | Paneles (nivel 2) |
| `dark.surface.card` | `#1E2028` | Tarjetas e inputs (nivel 3) |
| `dark.surface.raised` | `#242630` | Elementos elevados / hover |
| `dark.border.subtle` | `#30323D` | Bordes y separadores |
| `dark.text.primary` | `#F1F0F7` | Texto principal |
| `dark.text.soft` | `#E3E2EB` | Texto secundario fuerte |
| `dark.text.secondary` | `#A1A1B1` | Labels y metadatos |
| `dark.text.muted` | `#8A8CA0` | Texto atenuado (AA ≥ 4.5:1) |
| `dark.text.onAccent` | `#17181C` | Tinta sobre relleno de acento |
| `dark.accent.default` | `#8978FF` | Marca: rellenos, foco, checks |
| `dark.accent.text` | `#A99DFF` | Acento como texto (AA) |
| `dark.accent.surface` | `#28243A` | Fondo tintado de acento |
| `dark.accent.selection` | `#2A263D` | Selección |
| `dark.accent.active` | `#302C47` | Elemento activo |
| `dark.state.success` | `#61D7A2` | Estados correctos |
| `dark.state.warning` | `#F5B83D` | Avisos |
| `dark.state.danger` | `#F0736E` | Errores |
| `dark.elevation.low` | `0 2 8 #000` | Sombra baja |
| `dark.elevation.mid` | `0 6 18 #000` | Sombra media |
| `dark.elevation.high` | `0 14 36 #000` | Sombra alta |

### Theme/Light (22 tokens)

| Token | Valor | Uso |
| --- | --- | --- |
| `light.surface.app` | `#F6F5F2` | Fondo de ventana (blanco roto) |
| `light.surface.bubble` | `#F0EFEC` | Superficie de burbuja |
| `light.surface.panel` | `#FFFFFF` | Paneles (nivel 2) |
| `light.surface.card` | `#FFFFFF` | Tarjetas e inputs (nivel 3) |
| `light.surface.raised` | `#EBEBEF` | Elementos elevados / hover |
| `light.border.subtle` | `#D9D9E0` | Bordes y separadores |
| `light.text.primary` | `#17181C` | Texto principal |
| `light.text.soft` | `#2E3038` | Texto secundario fuerte |
| `light.text.secondary` | `#55565F` | Labels y metadatos |
| `light.text.muted` | `#6E6E7A` | Texto atenuado (AA ≥ 4.5:1) |
| `light.text.onAccent` | `#17181C` | Tinta sobre relleno de acento |
| `light.accent.default` | `#8978FF` | Marca (gráficos ≥ 3:1) |
| `light.accent.text` | `#6652F2` | Acento como texto (AA) |
| `light.accent.surface` | `#ECE9FF` | Fondo tintado de acento |
| `light.accent.selection` | `#E2DDFF` | Selección |
| `light.accent.active` | `#D6CFFF` | Elemento activo |
| `light.state.success` | `#1F7A3E` | Estados correctos |
| `light.state.warning` | `#8F6300` | Avisos |
| `light.state.danger` | `#C93E3E` | Errores |
| `light.elevation.low` | `0 2 8 #D9D9E0` | Sombra suave baja |
| `light.elevation.mid` | `0 6 20 #C9C9D4` | Sombra suave media |
| `light.elevation.high` | `0 16 48 #B8B8C6` | Sombra suave alta |

Contraste WCAG verificado por cálculo sobre los valores reales: todos los
pares texto/fondo ≥ 4.5:1 (mínimo: `light.text.muted` sobre `light.surface.app`
= 4.61:1; tinta sobre acento = 5.23:1). Los valores `dark.text.muted` y
`light.text.muted` suben respecto al Núcleo previo (`#737585`, que medía
~3.6:1 sobre tarjeta) precisamente para cumplir AA.

### Theme/Custom (6 tokens — reemplazo en runtime)

| Token | Tipo | Default | Rango / notas |
| --- | --- | --- | --- |
| `surface-user` | color | `#212436` | Superficie base elegida por el usuario |
| `bubble-user` | color | `#6C63FF` | Burbuja de turnos dictados |
| `bubble-app` | color | `#2E3350` | Burbuja de sistema/estado |
| `bubble-opacity` | opacity | `0.90` | 0.20–1.00 (20–100 %) |
| `bg-image-overlay` | opacity | `0.55` | Capa sobre la imagen de fondo |
| `bubble-radius` | borderRadius | `18` px | Radio de burbuja del usuario |

### Escalas estructurales (añadidas a `Whispr Open · Núcleo`)

| Token | Valor |
| --- | --- |
| `space.xs` / `space.sm` / `space.md` | 4 / 8 / 16 px |
| `space.lg` / `space.xl` / `space.2xl` | 24 / 32 / 48 px |
| `shape.radius.control` | 8 px |
| `shape.radius.bubble.square` | 4 px |
| `shape.radius.bubble.soft` | 12 px |
| `shape.radius.bubble.rounded` | 20 px |
| `font.family.sans` | Inter |

Preexistentes del Núcleo (sin cambios): `font.size.*` 8/9/10/11/12/13/15/17/
18/27/30, `shape.radius.card` 10, `shape.radius.pill` 999,
`shape.border.hairline` 1.

## 4. Mapa token → opción de personalización

| Token Custom | Opción del usuario | Campo en código |
| --- | --- | --- |
| `surface-user` | Color de superficie | `Config::palette` (`PaletteConfig`, overrides por campo) |
| `bubble-user` | Color de burbuja de turno | `Config::palette` (campo nuevo `bubble_user`) |
| `bubble-app` | Color de burbuja de sistema | `Config::palette` (campo nuevo `bubble_app`) |
| `bubble-opacity` | Opacidad de burbuja | `Config::bubble_opacity` (u8 20–100, default 100) |
| `bg-image-overlay` | Opacidad sobre imagen de fondo | `Config::bg_image_path` + overlay (campo nuevo `bg_image_overlay`) |
| `bubble-radius` | Forma de burbuja | Campo nuevo `bubble_radius` (hoy hay estilos `BubbleStyle::{Solid,Ring,Minimal}`) |

Resto de tokens → código: `dark.*`/`light.*` se corresponden con `theme.rs`
(`UiPalette`); las diferencias ya medidas se listan en §2 y se resumen:
acento de marca `#8978FF` (vs `#7C6CFF` en código — el archivo manda sobre
el código, migración pendiente), tinta sobre acento `#17181C` =
`INK_ON_ACCENT`, `accent.text` = `ACCENT_TEXT`/`ACCENT_TEXT_LIGHT`, estados =
`OK`/`WARN`/`ERR` con los valores del archivo. `space.*` = `SPACE_1..6`
(4/8/16/24/32/48 idénticos), `shape.radius.card` = `RADIUS_MD` (10).

## 5. Métricas del sistema de burbujas (Fase 3 — implementadas y verificadas)

Valores reales del archivo tras revisión/reparación (texto con envoltura
`auto-height` + `fill` horizontal en flex; contenedores con altura `auto`):

| Métrica | Valor |
| --- | --- |
| Ancho máximo de burbuja | 520 px Normal · 340 px Compacto (ventana 480/390 con padding 16) |
| Ancho por defecto en prototipos | 240 px Normal · 200 px Compacto · 220 px agrupadas · 320 px sistema |
| Padding interno | 12 px vertical × 16 px horizontal (Normal) · 8 × 12 (Compacto) · 8 × 14 (sistema) |
| Separación entre burbujas | 8 px entre turnos · 2 px visual dentro de grupo · 4 px gap interno (línea/meta) |
| Altura mínima | 40 px (agrupadas/una línea) · 58–66 px estados · 30 px compacta · 31 px sistema |
| Tipografía | 13 px texto de turno Normal · 11 px Compacto · 10 px meta · 11 px sistema |
| Radios de forma | `bubble.rounded` 20 px · `bubble.soft` 12 px · `bubble.square` 4 px · sistema 999 px (pill) |
| Agrupación primera/media/última | primera: radio grande salvo abajo-izq; media: 4 px uniforme; última: 4 px salvo abajo-izq |
| Estados visuales | transcribiendo: 3 puntos de acento + «transcribiendo…»; error: borde `state.danger` 1 px; edición: etiqueta «editando…» |
| Estados de sistema | pill 320×31: `model-loading` ◌, `mic-paused` ⏸, `offline` ⌁ |

También se revisó `02 · Componentes` y se corrigió el board «C · Widgets»
(slider y cabecera de panel a 320 px de ancho real, carril del slider
centrado: 0 desbordes verificados).

## 6. Estado de las fases de diseño en Penpot (a 14/10/2026)

- ✅ Fase 0: inventario del archivo y medidas reales (§1–2).
- ✅ Fase 1 (tokens): sets + themes creados y verificados (§3).
- ✅ Fase 2 (`02 · Componentes`): 25 componentes locales con variantes y
  auto-layout; defectos de anchos/desbordes corregidos (C · Widgets).
- ✅ Fase 3 (`03 · Burbujas`): sistema completo A–G; réparado el defecto
  principal (textos de una línea con burbujas de 100 px fijos → envoltura a
  ancho real y altura auto en componentes E instancias).
- ✅ Fase 4: páginas nuevas creadas:
  - `04 · Dark`: Ventana normal 480×800 · Ventana compacto 390×760 · Ventana vacía 480×800.
  - `05 · Light`: los mismos boards con valores light.* (recoloreado dirigido verificado).
  - `06 · Custom`: ventana `surface-user` + degradado overlay `bg-image-overlay` 55 %,
    burbujas usuario `bubble-user` (radio 18, opacidad 0.9) y sistema `bubble-app`,
    + Panel de personalización 340×800 (selector de fondo,slider de opacidad,
    formas con vista previa, swatches, preview en vivo).
- ⛔ Fase 5 parcial: este documento actualizado; limpieza de páginas legacy
  (01 · Vistas principales / 02 · Ajustes / 03 · Referencia · Estilo) pendiente
  de decisión del usuario (no se borra nada sin avisar).
- Librerías conexión verificadas: Open Color Scheme y FontAwesome conectadas;
  Essential UI y 100 card templates siguen sin API de conexión (manual).
