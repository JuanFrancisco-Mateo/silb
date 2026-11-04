# Paleta de colores (Ajustes → Apariencia)

## El fallo y su causa

"Cambiar colores no funciona para nada": el hex se guardaba en
`config.json`, pero la ventana no cambiaba salvo en los textos que la vista
calcula por su cuenta.

La causa está en `src/theme.rs`: la paleta activa viaja en un `thread_local`
(`OVERRIDE`) que `WhisprApp::view()` ponía con un `PaletteOverrideScope` que
**se soltaba al volver de `view()`**. iced evalúa los estilos de los widgets
(tarjetas, botones, campos) al **dibujar**, o sea *después*: cuando corrían,
`tokens(theme)` ya no encontraba paleta y devolvía los colores de serie. Y el
`Theme` de la ventana principal se pedía con `theme::custom(dark)`, que tampoco
lleva la paleta del usuario.

Prueba (log de la app, `log!` temporal dentro de `tokens()`, build anterior al
arreglo, con `"accent": "#00AAFF"` en el config):

```
[palette] PRUEBA tokens: override=true  theme_primary=Color { r: 0.0, g: 0.667, b: 1.0, a: 1.0 }   ← dentro de view()
[palette] PRUEBA tokens: override=false theme_primary=Color { r: 0.486, g: 0.424, b: 1.0, a: 1.0 }  ← al dibujar (violeta de serie)
```

El mismo frame tenía la paleta del usuario y, unas líneas después, la de serie.

## El arreglo

- `WhisprApp::view()` deja la paleta puesta con `theme::set_palette_override(...)`
  y **no la suelta**: el dibujo del frame llega después.
- El `Theme` de la ventana principal sale de `WhisprApp::theme()`, que ya
  resolvía la paleta (`custom_with_palette`), en vez de `theme::custom(dark)`.
- Selector con el ratón: cada campo de la paleta lleva una fila de **muestras
  clicables** (`PaletteField::swatches`) que aplica y guarda ese color al
  instante. El hex de siempre se queda como opción avanzada.
- `PaletteConfig::set_field` centraliza el mapa campo → hueco (lo usan el hex,
  el selector y los presets).

## Capturas

| Fichero | Qué enseña |
|---|---|
| `paleta-antes.png` | Paleta de serie: fondo `#17181C`, acento violeta `#7C6CFF` |
| `paleta-despues.png` | Tras pulsar las muestras `#3A3C45` (fondo) y `#00AAFF` (acento), y reiniciar la app |
| `paleta-selector-muestras.png` | La tarjeta "Paleta" con la fila de muestras de cada campo |

Comprobado al píxel: el fondo pasa de `#17181C` a `#3A3C45` (249 303 px) y el
acento de `#7C6CFF` a `#00AAFF` (4 714 px, botón del micrófono, logo y botones
primarios). Tras cerrar y abrir la app, la captura es idéntica: la paleta se
guarda en `config.json` (`"palette": { "bg": "#3A3C45", "accent": "#00AAFF", … }`).
