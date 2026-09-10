# Guía de usuario — Whispr Open

Versión: 0.2.0 · Última revisión: 10/09/2026 (verificada contra binario release)
y la especificación `docs/CORRECCION-VOZ.md`.

Whispr Open es un dictado por voz local: hablas, la app transcribe y escribe
el texto donde tengas el cursor. Además, **corrige lo que dices mientras
dictas**: si te equivocas, lo arreglas hablando.

---

## 1. Empezar

1. Arranca `whispr-open.exe` (`target\release\whispr-open.exe`).
2. Espera a la pantalla de carga: descarga el modelo de voz la primera vez
   (~75 MB tiny / ~142 MB base / ~466 MB small) y, si la corrección IA está
   activada, lanza el server LLM local (primera carga: unos segundos).
3. Comprueba el chip de estado bajo el header:
   - `● Corrección IA: activa · <modelo>` (verde) → listo para corregir.
   - `○ Corrección IA: inactiva — <motivo>` (ámbar) → dicta igual, sin corregir.
   - `○ Corrección IA: apagada` (gris) → actívala en Ajustes → Corrección IA.

## 2. Dictar

1. Pulsa **Ctrl+Shift+D** (o el botón ● Grabar). Suena un «clic».
2. Habla con naturalidad.
3. Vuelve a pulsar **Ctrl+Shift+D** (o suelta). La app transcribe y escribe el
   texto en la app que tenga el foco (Slack, Notion, VS Code…).

Si «Autoinyectar» está desactivado, el texto queda en la ventana de Whispr y
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

## 4. Diccionario personal

Para jerga, siglas y nombres propios (nombres de la empresa, anglicismos…):
Ajustes → Diccionario. Cada entrada es una palabra que Whisper no transcribe
bien y que la app intenta respetar en el dictado.

## 5. Si algo va mal

- **El texto no se inyecta en la app**: comprueba que «Autoinyectar» está
  activo y que la app destino tiene el foco. Si tienes un filtro por título de
  ventana (`inject_window_filter`), el título debe contenerlo.
- **Quieres ver qué ha decidido la app**: el log está en
  `%APPDATA%\whispr-open\whispr-open.log` (registra clasificación y enrutado de
  cada turno).
- **La corrección consume memoria**: el toggle de Corrección IA apagado
  descarga el modelo LLM de la RAM/VRAM. Al salir de la app, el server LLM se
  cierra solo (no queda ningún `llama-server.exe` huérfano).
- **Informes de fallo útiles**: versión del binario, chip de estado, y el
  trozo del log del turno que falló.

## 6. Referencias

- Configuración de Local/Custom/Cloud: `docs/SETUP.md`.
- Diseño del corrector: `docs/CORRECCION-VOZ.md`.
- Configuración avanzada (`config.json`): README → Configuración avanzada.