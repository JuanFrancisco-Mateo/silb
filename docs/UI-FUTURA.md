# Diseño e ideas — lo que ya está y lo que queda

Este documento cuenta qué partes del diseño están cerradas (para no reabrirlas
sin motivo), qué falta y qué ideas hay para las próximas versiones. Los colores y
las medidas viven todos en `src/theme.rs`.

---

## 1. Lo que ya está (no rehacer)

- **Modo oscuro por defecto**, con interruptor que se recuerda entre arranques.
- **Medidas centralizadas** en `src/theme.rs`: un solo acento violeta
  (`#7C6CFF`), rejilla de 8 px, radios de 8–10 px y una escala de tamaños de
  letra. En la interfaz no hay valores sueltos.
- **Una pantalla, una pregunta** («¿qué estoy dictando?») y dos secciones en la
  ventana principal —**Dictado** y **Ajustes**— con la misma barra de pestañas.
- **Estado del micrófono por dos vías**: color y texto, con medidor de nivel.
  Sin animaciones de adorno, sin cristal translúcido ni relieves falsos.
- **El botón grande del micrófono** como centro de la pantalla.

## 2. Transparencia con la IA

Si la app usa IA, quien la usa tiene que **ver** qué hace y poder corregirlo.

- **Aviso de confianza a la vista**: «Transcripción local · nada sale de tu PC»
  en el pie de la ventana. Si la corrección usa un servicio en la nube, el texto
  cambia y no promete nada: hay un test que lo comprueba.
- **Texto editable**: lo transcrito es un editor de varias líneas; puedes
  corregirlo a mano y seguir dictando sin que una cosa pise la otra (lo nuevo se
  añade al final del texto editado).
- **Enseñar lo que se cambió**: la corrección por voz muestra el antes y el
  después y ofrece Deshacer (ver `CORRECCION-VOZ.md`).

## 3. Contraste y accesibilidad

- **Contraste del acento**: se calcula el contraste WCAG en el propio código y
  hay tests que exigen ≥ 4,5:1 en texto y ≥ 3:1 en gráficos. El violeta de texto
  es algo más claro que el de marca para cumplirlo.
- **Menos movimiento**: no hay animaciones de adorno que desactivar. Lo único
  que se actualiza a menudo es el latido de estado (100 ms) y la animación por
  frames de la burbuja, que es material propio del usuario y solo se programa si
  lo configura. Si algún día se añade un fundido, debe mirar antes la preferencia
  de menos movimiento del sistema.
- **Foco de teclado visible**: los campos de texto y el editor dibujan un anillo
  de acento al enfocar. En botones, casillas y desplegables, iced 0.13 no expone
  el estado de foco; cuando lo haga, se añade.
- **Zonas para pulsar**: el círculo grande y la burbuja (56 px) son grandes por
  diseño; los botones secundarios usan el margen para dedo del tema en vez del
  margen por defecto del framework.

## 4. Estado y avisos

- **Errores en castellano y accionables**: los fallos de descarga, micrófono y
  escritura se cuentan en una frase corta; el detalle técnico queda en el
  registro.
- **Descarga del modelo con contexto**: además del porcentaje se ve el tamaño
  descargado y el tiempo estimado que queda.
- **Nivel de micrófono con marca de pico**: la barra dibuja el pico real y avisa
  cuando importa — «sin señal del micrófono: ¿está silenciado?» pasados unos
  segundos, y «el micrófono puede estar saturando» cuando se pasa de rango. El
  aviso lleva texto, no solo color.

## 5. Ideas opcionales (de bajo coste)

- **Mascota de marca**: un estado pequeño (dormida / listo / grabando) en la
  burbuja o en el botón grande. Se puede quitar sin romper nada.
- **Pantalla sin texto** con una ayuda visual discreta, además del ejemplo que
  ya sale hoy.
- **Gesto en la burbuja**: doble clic para repetir el último turno, sin tocar la
  detección de arrastre actual.

## 6. Personalización con imágenes propias

iced 0.13 no trae un widget de imagen animada (ni GIF ni WebP de serie), así que
la app deja poner material propio:

- **Fondo de la ventana** (PNG, JPG, BMP, GIF, WebP o TIFF), dibujado por debajo
  del contenido y recortado para cubrir. Si la ruta no existe o el formato no se
  puede leer, vuelve al fondo del tema: nunca una ventana en negro.
- **Imagen fija de la burbuja**, con los mismos formatos y la misma caída al
  dibujo normal si falla.
- **Animación por frames**: una carpeta de imágenes que se van cambiando a un
  ritmo configurable (1–30 por segundo, 12 por defecto), en orden alfabético y en
  bucle. Si la carpeta está vacía, se usa la imagen fija.

Lo elegido se guarda como **copia** dentro de los datos de la app
(`%APPDATA%\whispr-open\assets\` en Windows, `~/.local/share/whispr-open/assets/`
en Linux), no como ruta al fichero original: si vacías la carpeta de descargas,
tu personalización sigue ahí. Una ruta antigua que aún exista se respeta; al
arrancar se descartan las que ya no llevan a nada.

## 7. Registro con rotación

El registro llegó a 1,14 MB y 11.102 líneas, de las que el 79 % eran una línea
por cada movimiento del ratón al arrastrar la burbuja. Ahora:

- **Rota por tamaño** (`src/log.rs`, solo biblioteca estándar): al pasar de ~1 MB
  el fichero se convierte en `.1` y el `.1` en `.2`, y se sigue escribiendo en
  uno nuevo.
- Las trazas de mucha frecuencia (`drag start` / `drag target`) son de nivel
  **debug**: solo se escriben con `WHISPR_DEBUG=1`.
- La ruta del registro se apunta al arrancar y **Ajustes → Avanzado** la enseña,
  con un botón para abrir la carpeta (junto al estado de `WHISPR_DEBUG` y a dónde
  viven las imágenes propias).

## 8. Próximas ideas — elegir el modelo de IA desde Ajustes (0.7)

Hoy, en **Ajustes → Corrección IA**, el modelo local se escribe a mano en un
campo de texto. La idea es que pase a ser un **desplegable** (o un menú) con
modelos de **Hugging Face**, y que **la app los descargue** sola, como ya hace
con el modelo de voz. Nada de ir a la web a bajar el fichero y dejarlo en la
carpeta `models-llm`.

Decisiones pendientes antes de ponerse:

- ¿Desplegable con una **lista corta elegida por el proyecto** (los recomendados
  que ya aparecen en esa pantalla: Qwen2.5-0.5B, Llama 3.2 1B, Qwen2.5-1.5B) o
  un **buscador libre** de Hugging Face donde se pega el nombre del repo o la
  URL?
- Si alguien elige un modelo que **no es GGUF** (safetensors, por ejemplo),
  ¿se avisa y se descarta? Hoy tanto whisper como el servidor de corrección
  trabajan solo con GGUF.

Encaja con lo que ya existe (`CORRECCION-VOZ.md`): el modelo local se resuelve
por nombre dentro de `models-llm`; esto añade delante el paso de elegirlo y
traerlo, sin tocar el resto del camino.

## 9. Criterios de aceptación

- Todo color, espacio y radio sale de `src/theme.rs`; un valor nuevo se añade
  ahí, nunca suelto en `app.rs`.
- El aviso de confianza y el texto editable respetan el modo claro y el oscuro.
- Sin animaciones de entrada gratuitas: solo respuestas de estado cortas
  (150–250 ms) y respetando la preferencia de menos movimiento.
- Mostrar solo lo necesario: lo nuevo se añade al panel o al botón grande, sin
  llenar la ventana de barras.
- `cargo test` y `cargo check` en verde, con los tests de contraste intactos.
