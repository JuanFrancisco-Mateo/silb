# Guía de usuario — Silb

Versión: 0.2.0 · Última revisión: 24/09/2026 (secciones de la ventana
principal, imágenes propias que ya no se pierden al vaciar Descargas y dónde
vive el registro; antes: 11/09/2026, paleta editable) y la especificación
`docs/CORRECCION-VOZ.md`.

Silb es un dictado por voz local: hablas, la app transcribe y escribe
el texto donde tengas el cursor. Además, **corrige lo que dices mientras
dictas**: si te equivocas, lo arreglas hablando. Y si prefieres tocar el
texto con el teclado, **la nota se puede editar a mano** antes de mandarla.

---

## 1. Empezar

1. Arranca `silb.exe` (`target\release\silb.exe`).
2. Espera a la pantalla de carga: descarga el modelo de voz la primera vez
   (~75 MB tiny / ~142 MB base / ~466 MB small) y, si la corrección IA está
   activada, lanza el server LLM local (primera carga: unos segundos).
3. Comprueba el chip de estado bajo el header:
   - `● Corrección IA: activa · <modelo>` (verde) → listo para corregir.
   - `○ Corrección IA: inactiva — <motivo>` (ámbar) → dicta igual, sin corregir.
   - `○ Corrección IA: apagada` (gris) → actívala en Ajustes → Corrección IA.
4. La ventana tiene **dos secciones** en la barra de arriba: **Dictado** (el
   día a día: micrófono, medidor y tu nota) y **Ajustes** (todo lo demás). La
   sección activa va marcada con `●`. Cambiar de sección no pierde nada de lo
   que hayas escrito.

## 2. Dictar

1. Pulsa **Ctrl+Shift+D** (o el botón ● Grabar). Suena un «clic».
2. Habla con naturalidad. La barra del micrófono pinta el **pico** del turno
   (`pico N%`) y te avisa si algo va mal ahí mismo: «pico al máximo: el
   micrófono puede estar saturando» o «sin señal del micrófono: ¿está
   silenciado?».
3. Vuelve a pulsar **Ctrl+Shift+D** (o suelta). La app transcribe y escribe el
   texto en la app que tenga el foco (Slack, Notion, VS Code…).

Si «Autoinyectar» está desactivado, el texto queda en la ventana de Silb y
puedes usar «Escribir en app» o «Copiar».

## 3. Corregir hablando (lo importante)

La app entiende dos maneras de corregirte, y ambas funcionan sin tocar nada
más: sigues dictando y te corriges en voz alta.

### 3.1 Corregirte sobre la marcha (mismo chorro de voz)

Dices algo y te arreglas en la misma frase — lo que sobre escribe el error:

> «que calor que hace en murcia **digo cartagena**»
> → escribe: *que calor que hace en cartagena*

Frases que reconoce:
- **digo X / diga X / dime X**: «el cliente es Acme **quiero decir** Microsoft»
- **quiero decir X / quise decir X / mejor dicho X**: «quedamos a las tres,
  **mejor dicho** a las cuatro»
- **en vez de / en lugar de**: «voy a las tres **en vez de** las cuatro»
- **mejor X que Y / no X sino Y**: «no el martes **sino** el jueves»

Regla mental: di la versión mala, di el marcador, di la versión buena. La app
elimina el marcador y las palabras que lo rodean.

### 3.2 Corregir el turno anterior (cross-turn)

Te refieres a lo que acabas de dictar:

> «la reunión es a las **4** en punto» → al siguiente turno: «**no no lo
> último está mal, es 2 en vez de 4**»
> → el texto queda: *la reunión es a las 2 en punto*

Frases que reconoce:
- **no / mal / me equivoqué** + referencia al texto: «no, eso no, era 5»
- **referencias** a lo anterior: «lo último», «ese número», «la última frase»
- **verbos correctivos**: «corrige», «cambia», «pon», «quita», «borra»
- **numeral + en vez de**: «es 2 en vez de 4»

### 3.3 Si la corrección no se aplica

La corrección IA prioriza no perder texto: ante la duda, **inserta lo que has
dictado sin corregir** (con la meta-lengua incluida). Motivos típicos:

| Síntoma | Causa probable | Qué hacer |
|---|---|---|
| Inserta «digo X» literalmente | El clasificador no vio señal clara, o el LLM no llegó a tiempo | Repite la corrección más explícita («no, quería decir X») o revisa el chip de estado |
| El chip dice «inactiva — …» | Server LLM no disponible | Ajustes → Corrección IA → Server LLM (o activa la corrección) |
| Corrige a veces, otras no | Sensibilidad del clasificador (default 50) | Sube **Sensibilidad** en Ajustes avanzados para que enrute más casos al LLM |
| Se inserta sin corregir tras esperar | Timeout (default 25 s) superado | Sube **Espera máx. (s)** en Ajustes avanzados |
| El modelo devolvió un resumen | Guard anti-truncación descartó la respuesta | Nada: se mantiene tu texto original (nunca se pierde) |

Si quieres decidir tú antes de aplicar: Ajustes → Corrección IA → «Confirmar
antes de aplicar» (muestra «Aplicar / Descartar»). Hay «Deshacer ↩» de 1
nivel por si una corrección se aplicó mal.

## 4. Corregir a mano (la nota es editable)

El panel central de la ventana es un **editor de texto**: haz clic y corrige
lo que quieras con el teclado, como en cualquier nota.

- **Editar y seguir dictando no se pisan**: cada turno nuevo se añade al
  final de lo que ya hayas corregido a mano.
- **Copiar** confirma con un «✓ Copiado» breve en el propio botón. Si el
  portapapeles falla, verás el error en el panel superior.
- **Limpiar** pide confirmación («¿Borrar la nota? Sí / No») y el borrado
  se puede deshacer con «↩ Deshacer borrado» durante el turno siguiente.
- La corrección por voz («no, era X») sigue funcionando sobre lo último de
  la nota, incluido lo que hayas editado a mano.

Si «Autoinyectar» está desactivado, el texto queda en la ventana de Silb y
puedes usar «Escribir en app» o «Copiar».

## 5. Diccionario personal

Para jerga, siglas y nombres propios (nombres de la empresa, anglicismos…):
Ajustes → Diccionario. Cada entrada es una palabra que Whisper no transcribe
bien y que la app intenta respetar en el dictado.

## 6. Personalizar la apariencia (paleta)

Ajustes → Apariencia → Paleta te da control total sobre los colores de la
app. Cada campo es editable por separado o puedes usar un preset de un clic.

### 6.1 Presets

Cinco combinaciones listas para los dos modos:

- **Violeta (oscuro por defecto)**: el look de Silb tal como se
  distribuye.
- **Grafito (cian sobrio)**: tono grafito con acento cian; lectura larga,
  poco distractivo.
- **Claro (violeta)**: el modo claro con acento violeta.
- **Bosque (verde sobre crema)**: verde sobre fondo crema; ambiente cálido,
  lectura sin reflejos.
- **Oceánico (azul sobre tinta)**: azul profundo sobre tinta oscura;
  alternativa al grafito en oscuro, o azul sobre blanco en claro.

Cada preset reemplaza los 12 campos. Si después afinas uno o dos, el resto
del preset se mantiene.

### 6.2 Campos editables (12)

Cada fila tiene un campo hex (`#RRGGBB` o `#RGB`, mayúsculas o minúsculas,
con o sin `#`), un swatch con el color actual y un botón «Restaurar» que
devuelve ese campo al default del modo activo. Los 12 campos:

| Etiqueta              | Qué controla                                            |
|-----------------------|---------------------------------------------------------|
| Fondo de ventana      | Color de fondo de la ventana principal.                 |
| Tarjetas / inputs     | Color de las tarjetas y de los campos de texto.         |
| Hover / superficies 2 | Fondo al pasar el cursor sobre elementos.               |
| Texto principal       | Color principal del texto. Mide ≥ 4.5:1 sobre fondo.   |
| Texto atenuado        | Texto secundario (labels, pistas). Mide ≥ 4.5:1.        |
| Acento                | Color de marca (botones primarios, foco, Checks).       |
| Acento (hover)        | Acento al pasar el cursor sobre el botón primario.      |
| Acento (texto)        | Acento usado como texto normal (≥ 4.5:1 sobre fondo).   |
| Ok                    | Estados correctos (verde).                              |
| Aviso                 | Estados de aviso (ámbar).                               |
| Error                 | Estados de error (rojo).                                |
| Bordes                | Bordes sutiles de tarjetas y separadores.               |

Si el hex escrito no es válido, la app **lo ignora y usa el default** (no
rompe la UI, no muestra error modal). Al pulsar «Aplicar» se persiste el
valor en `config.json`.

### 6.3 Aviso de contraste

Al editar Texto principal o Texto atenuado, la fila muestra en ámbar el
ratio WCAG medido contra el fondo activo. Si baja de 3:1, sale un aviso
«⚠ Contraste X.XX:1 (texto sobre fondo, mínimo WCAG 3:1). Se aplica
igualmente; ajusta hex o restaura el campo». **No bloquea** al usuario:
solo avisa, porque hay contextos legítimos (modo claro experimental,
demostraciones).

### 6.4 Importar / Exportar JSON

- **Exportar JSON…**: escribe un fichero `.json` con la paleta actual
  resuelta (los 12 campos ya con defaults aplicados si los habías dejado en
  blanco). Pensado para compartir.
- **Importar JSON…**: lee un fichero y **funde** sus campos con tu paleta
  actual: un campo presente en el JSON sobrescribe el tuyo, uno ausente se
  mantiene. Esto permite compartir una paleta parcial (p. ej. solo el
  acento) sin perder tus ajustes.

Formato del JSON (los 12 campos; los ausentes se mantienen al importar):

```json
{
  "bg": "#17181C",
  "surface": "#1F2026",
  "surface_alt": "#272830",
  "text": "#F7F6F3",
  "dim": "#9A9AA3",
  "accent": "#7C6CFF",
  "accent_hover": "#8F82FF",
  "accent_text": "#8F82FF",
  "ok": "#3FB95F",
  "warn": "#F5A623",
  "err": "#EB5757",
  "border": "#2E3038"
}
```

### 6.5 Imágenes propias (fondo y burbuja)

Ajustes → Apariencia → Imágenes te deja poner tu propio material
visual. Tres controles, todos opcionales e independientes:

- **Fondo de la ventana**: una imagen estática (PNG/JPG/BMP/GIF/WebP/TIFF).
  Se estira para cubrir la ventana. Si la ruta no existe o el formato no
  es legible, la app cae al fondo del tema (no se queda en negro).
- **Sprite de la burbuja**: una imagen estática que reemplaza el círculo
  violeta de la burbuja flotante. La grabación (punto blanco) y la
  corrección IA (etiqueta «IA») siguen visibles encima del sprite.
- **Frames de animación**: una carpeta con PNGs (u otro formato
  soportado) que se conmutan como animación de la burbuja. Orden
  alfabético; wrap al llegar al último. La velocidad (FPS) la eliges tú
  con un slider (1–30, default 12). Si la carpeta está vacía, la
  animación cae al sprite estático o al render vectorial.

**Cómo se eligen**: «Elegir…» abre el diálogo nativo de Windows. Al elegir,
la app **copia** tu material a su carpeta de datos
(`%APPDATA%\whispr-open\assets\`) y guarda ESA copia en `config.json`: puedes
vaciar Descargas sin perder la personalización (antes se guardaba la ruta al
fichero original y la personalización desaparecía en silencio). Se aplica al
instante. «Quitar» limpia la ruta y vuelve al aspecto original. Si la ruta
guardada ya no existe (por ejemplo, borraste la copia a mano), la app lo dice
en el registro («asset desaparecido, cayendo al tema»), cae al fallback sin
crash y no deja la ruta muerta en la configuración.

**GIF/WebP animado nativo**: iced 0.13 no lo soporta (verificado en
`docs/UI-FUTURA.md` §7). Por eso la animación aquí se hace por
conmutación de frames PNG: pierdes la comodidad de un único fichero,
pero ganas control fino sobre la velocidad y el orden. Cuando la app
migre a iced 0.14+, este control se reemplazará por el widget nativo.

## 7. Si algo va mal

- **El texto no se inyecta en la app**: comprueba que «Autoinyectar» está
  activo y que la app destino tiene el foco. Si tienes un filtro por título de
  ventana (`inject_window_filter`), el título debe contenerlo. Si no había
  NINGUNA ventana con el foco, el texto queda copiado en el portapapeles con
  un aviso: pégalo tú donde quieras.
- **El micrófono se desconecta a mitad de una nota**: la app corta el turno
  sola, transcribe lo que oyó hasta ese momento y te avisa. Reconecta el
  micro y sigue dictando: no hace falta reiniciar.
- **Quieres ver qué ha decidido la app**: el log está en
  `%APPDATA%\whispr-open\whispr-open.log` (registra clasificación y enrutado de
  cada turno) y **Ajustes → Avanzado** te muestra su ruta exacta con un botón
  «Abrir carpeta del registro». El fichero **rota solo**: al pasar de 1 MB
  pasa a `.1` y el `.1` a `.2`, así que nunca crece sin límite. Las trazas muy
  detalladas (cada movimiento al arrastrar la burbuja) solo se escriben si
  arrancas la app con la variable de entorno `WHISPR_DEBUG=1`.
- **La corrección consume memoria**: el toggle de Corrección IA apagado
  descarga el modelo LLM de la RAM/VRAM. Al salir de la app, el server LLM se
  cierra solo (no queda ningún `llama-server.exe` huérfano).
- **Doble clic en el exe con la app ya abierta**: aparece un aviso («Ya hay
  otra instancia…») y la segunda copia se cierra sola: el atajo global no se
  duplica ni se roba.
- **Informes de fallo útiles**: versión del binario, chip de estado, y el
  trozo del log del turno que falló.

El pie de la ventana recuerda siempre dónde está tu voz: «Transcripción
local · nada sale de tu PC». Si activas la corrección en la nube, el pie lo
dice («la corrección IA usa la nube») — nunca te promete una privacidad que
no estás usando.

## 8. Referencias

- Configuración de Local/Custom/Cloud: `docs/SETUP.md`.
- Diseño del corrector: `docs/CORRECCION-VOZ.md`.
- Configuración avanzada (`config.json`): README → Configuración avanzada.