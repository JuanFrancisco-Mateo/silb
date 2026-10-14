# Diseño de Figma — Whispr Open (Open Whispr)

Exportado del archivo Figma con el **MCP remoto oficial de Figma** (`https://mcp.figma.com/mcp`, OAuth)
el 14/10/2026. Cuenta: `jf.profesional.ai@gmail.com` (plan Starter) — el servidor remoto está disponible en
todos los planes, no hace falta Dev Mode ni el MCP de escritorio.

- Archivo: `https://www.figma.com/design/zNDTFSrM54LM99t94PjMFP/Sin-t%C3%ADtulo`
- `fileKey`: `zNDTFSrM54LM99t94PjMFP`
- Frames (página 0:1 «Page 1»), 1200x820 cada uno:
  - `3:484` **Whispr Open · Dictado**
  - `3:604` **Whispr Open · Ajustes**

## Contenido

| Fichero | Qué es |
|---|---|
| `raw/dictado-design-context.txt` | Código Tailwind+React del frame **Dictado**, generado por la tool `get_design_context`. Cada `div` lleva `data-node-id` y `data-name`, así que es el mapa capa→estilo más fiable. |
| `raw/dictado.png` | Render del frame Dictado (1024x700; Figma sirve 1024 de ancho). |
| `raw/ajustes-design-context.txt` | Igual, frame **Ajustes**. |
| `raw/ajustes.png` | Render del frame Ajustes. |
| `raw/variables-figma.txt` | `{}` — el archivo **no usa variables de Figma**: los colores van literales en cada capa. |
| `raw/metadata-arbol-capas.txt` | Árbol de capas (id, nombre, x, y, ancho, alto) de los dos frames. Es el inventario de componentes y las medidas exactas. |
| `raw/TOKENS.md` | Colores y tamaños de fuente con recuento de usos, extraídos por script del design context. |
| `raw/icons/*.svg` | 31 SVG vectoriales descargados del propio Figma (los `img*` que referencia el código). Nombres con sufijo del hash original. Son iconos tipo lucide. |
| `tools/figma_*.py` | Pipeline de extracción reproducible (ver abajo). |

Las URLs de assets que aparecen en los design context (`figma.com/api/mcp/asset/...`) son **efímeras**: para
re-descargar cualquier asset hay que volver a llamar al MCP (`get_screenshot`/`download_assets`), no guardar
las URLs. Los SVG de `icons/` ya están en disco.

## Tokens

### Color (uso real en el diseño → token actual en `src/theme.rs`)

| Figma | Uso | `theme.rs` hoy |
|---|---|---|
| `#17181c` | Fondo de ventana | `BG` (ya coincide) |
| `#131418` | Sidebar | — nuevo |
| `#15161a` | Window bar (titlebar propia) | — nuevo |
| `#1d1e24` | Tarjetas | `SURFACE` (`#1F2026`, ajustar) |
| `#191a20` | Campos y selects | — nuevo |
| `#24252d` | Keycaps de atajo, badges | — nuevo |
| `#282338` | Nav item activo | — nuevo |
| `#27282f` | Divider / bordes internos | `BORDER` (`#2E3038`, ajustar) |
| `#303139` | Borde de tarjeta y de campo | `BORDER` |
| `#f1f0f5` | Texto primario | `TEXT` (`#F7F6F3`, ajustar) |
| `#a9a8b5` | Texto secundario | `DIM` (`#9A9AA3`, ajustar) |
| `#7f808d` | Texto tenue (labels, versión) | — nuevo |
| `#7c6cff` | Acento (botón grabar, botón primario) | `ACCENT` (ya coincide) |
| `#8d7aff` | Acento activo (tab activa, cursor de texto, enlaces) | `ACCENT_HOVER`/`ACCENT_TEXT` |

### Tipografía

Inter (`Regular`, `Medium`, `Semi_Bold`). Tamaños medidos: 9, 10, 11, 12, 13, 14, 18, 19, 22, 28 px —
los más usados 11 px (20 usos) y 12 px (15). Hoy la app no bundlea Inter y resuelve los glifos con la fuente
de sistema + `Segoe UI Emoji` (`src/fonts.rs`).

### Iconografía

31 SVG en `raw/icons/`. Hoy la UI dibuja símbolos con glifos emoji (✓ ☀ ☾ ▾ ● ✕) desde `src/fonts.rs`.

## Regenerar / actualizar la extracción

Requiere el server `figma` en `hermes mcp ls` y su token OAuth cacheado en
`%LOCALAPPDATA%/hermes/mcp-tokens/figma.json`. El intérprete es el venv de Hermes (trae el SDK `mcp`).

```bash
PY="C:/Users/usuario/AppData/Local/hermes/hermes-agent/venv/Scripts/python.exe"
KEY=zNDTFSrM54LM99t94PjMFP

# listar tools y esquemas
$PY tools/figma_mcp.py --list
# volcar frames (design context + captura + metadata)
$PY tools/figma_dump.py $KEY raw '[
 {"tool":"get_design_context","args":{"nodeId":"3:484","clientLanguages":"rust,css"}},
 {"tool":"get_metadata","args":{"nodeId":"0:1"}}]'
# bajar los SVG referenciados + recalcular TOKENS.md
$PY tools/figma_export.py raw raw
```

`raw/01..05` son los ficheros que produce `figma_dump.py`; `TOKENS.md` e `icons/` los produce `figma_export.py`.

## Qué NO trae

- Variables/estilos de Figma (el archivo no los tiene: todo literal por capa).
- Ficheros de fuente Inter (hay que bundlearlos aparte, licencia OFL).
- Componentes publicados ni Code Connect (el archivo no publica librería).
- Estados de hover/press animados: el diseño son dos frames estáticos por pantalla, sin variantes.
