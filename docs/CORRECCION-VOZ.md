# CORRECCIÓN / FORMATEO POR VOZ — especificación y diseño final

Versión de diseño: **10/09/2026** — cerrada tras t_9fbc886f (servidor de IA
nativo y ruta configurable), t_6b9348de (protección contra respuestas
recortadas), t_301476a8 (liberar la memoria), t_e25f8ab7 (opciones claras y
transparencia), t_c2b0a22f (proveedores Local/Custom/Cloud), t_1fc864ef
(protección numérica por código) y t_d64797c2 (esa protección ampliada a frases
naturales + la reconstrucción como texto realmente escrito en la app). Este
documento es la referencia de **cómo funciona hoy** el corrector con IA; los
nombres de archivos y funciones que se citan están comprobados contra el código
(rama `native-iced`).

Objetivo: que quien dicta pueda corregir lo que acaba de decir **hablando**, con
frases como el ejemplo de siempre:

> «no no lo último está mal, es 2 en vez de 4»

y que la app entienda que quieres corregir, encuentre el trozo que hay que
cambiar y lo arregle en el texto final.

Esto es **trabajo sobre el texto ya transcrito**: no toca la grabación del audio
(cpal) ni la decodificación de Whisper. Si el corrector falla, siempre queda el
texto tal cual — **el dictado nunca se bloquea ni pierde texto**.

---

## 1. Arquitectura: cascada de routing

En `app.rs`, `Message::TranscriptionDone` decide qué hacer con el turno recién
transcrito (`processed`) por orden, de lo más prioritario a lo menos:

1. **Reglas** (`src/domain/correction.rs`, `apply_correction` +
   `CorrectionSession`): si el turno encaja en un patrón fijo
   («no, quería decir X», «mejor dicho X», numeral + «en vez de»…), se aplica sin
   IA y el flujo termina aquí (coste ~0).
2. **IN-TURN** (modo principal): si `classify_in_turn(processed)` → la IA recibe
   el **turno completo** y devuelve el **texto final corregido** (sin las
   palabras que usaste para corregirte). No hace falta un turno anterior: la
   corrección va dentro del mismo chorro de voz («que calor en murcia **digo
   cartagena**»).
3. **CROSS-TURN** (modo antiguo, si lo anterior no vale): si
   `corrective::classify_with(processed, threshold) == NeedsLlm` y existe un
   `last_turn` anterior → la IA recibe `(last_turn, instruction)` (el texto a
   corregir y la frase con la que te corriges) y devuelve un diff JSON
   `{find, replace}`.
4. **PLAIN**: el resto → dictado normal, sin que la IA intervenga.

Condiciones comunes a los dos modos con IA (en `TranscriptionDone`):

```rust
let can_route_common = !rule_corrected && enabled && !llm_busy && llm_ready;
// enabled  = config.ai_correction_enabled()
// llm_ready = matches!(self.llm_status, LlmStatus::Ready { .. })
// llm_busy = self.llm_rx.is_some() || self.llm_pending.is_some()
```

Si un turno activa AMBOS clasificadores (caso mixto, por ejemplo «no no lo
último está mal es 2 en vez de 4», que tiene referencia a lo anterior Y «en vez
de»), gana el **in-turn** por prioridad.

El registro de `%APPDATA%\whispr-open\whispr-open.log` apunta el resultado de
cada clasificador, qué condiciones cortaron y la decisión que se tomó (sin
consola, el `println!` de antes no se veía en ningún sitio — fallo del
09/09/2026).

---

## 2. Proveedores: Local / Remoto (OpenAI) / Remoto (endpoint propio)

Selector `pick_list` en el primer nivel de Ajustes → Corrección IA
(`Message::CorrectionProviderChanged`). Enum en `src/config.rs`:

```rust
pub enum CorrectionProvider { Default, Cloud, Custom }
// serde rename_all = "lowercase"  →  "default" | "cloud" | "custom" en config.json
```

Etiquetas del selector (`CorrectionProvider::label()`, sprint final): quien usa
la app solo ve dos modos, Local o Remoto.

| Proveedor | Etiqueta en la UI | API key |
|---|---|---|
| `Default` | Local (en este equipo) | nunca se muestra |
| `Cloud` | Remoto (OpenAI) | obligatoria |
| `Custom` | Remoto (endpoint propio) | opcional |

**Default = Local** (`is_local()`): `correction_provider()` devuelve `Default`
cuando el campo vale `None` (las configuraciones antiguas sin ese campo no
cambian de comportamiento).

`Config::llm_settings()` arma la configuración real del corrector
(`llm::LlmConfig`) con valores por defecto según el proveedor, y todos se pueden
cambiar campo a campo:

| Proveedor | `base_url` default | `model` default | key |
|---|---|---|---|
| Default (Local) | `http://127.0.0.1:8081/v1` | `ornith-1.0-9b` | — |
| Cloud | `https://api.openai.com/v1` | `gpt-4o-mini` | sí |
| Custom | `http://127.0.0.1:8081/v1` | `ornith-1.0-9b` | si el server la pide |

Los campos `llm_base_url` y `llm_model` se comparten entre proveedores: si pones
un endpoint propio, Custom (o Cloud) lo usa tal cual; si están vacíos, se usa el
valor por defecto del proveedor activo. Los valores se limpian de espacios y un
campo vacío cuenta como «no escrito» (None → valor por defecto), nunca como un
valor inválido.

### Comprobación del endpoint en arranque / cambio de proveedor

En `app.rs::run_llm_phase` (fase de arranque, o relanzada al cambiar de
proveedor):

- **Endpoint remoto** (Custom/Cloud con un host que no es el propio equipo,
  `llm::is_local_endpoint` = false): **no hay servidor local que arrancar**. Se
  hace una petición `GET {base_url}/models` con 2 s de espera
  (`llm::server_alive_auth`), mandando `Authorization: Bearer <key>` **solo si hay
  key** (sin ella, un 401 haría creer que está caído cuando no lo está). Si
  responde → `LlmReady { launched: false }`; si no → `LlmDisabled(remote_endpoint_reason(url))`
  con un aviso a la vista.
- **Endpoint local**: si `/v1/models` ya responde → `LlmReady`; si no, se busca
  y se arranca `llama-server.exe` (ver §5) y se espera a que el modelo cargue
  (máximo 60 s). Si falla → `LlmDisabled(motivo)`.

`server_alive*` usa `/v1/models` y no `/health`: llama.cpp responde 503 en
`/health` mientras carga el GGUF, pero `/models` solo contesta cuando el modelo
está listo — es la señal fiable de «servidor vivo y modelo cargado».

Al cambiar de proveedor (`CorrectionProviderChanged`), la fase que estuviera en
marcha con la configuración vieja se cancela (con un token) y se relanza
`spawn_llm_startup()` para que el aviso de estado refleje el proveedor activo.
Cualquier fallo termina en `LlmDisabled(razón)`: la corrección se desactiva con
un aviso a la vista y el dictado sigue con el texto tal cual.

---

## 3. API key: Credential Manager, NUNCA config.json

El campo `llm_api_key` de `Config` lleva `#[serde(skip_serializing)]`: **no se
escribe en `%APPDATA%\whispr-open\config.json` en ningún caso** (así fue desde el
diseño original, sin cambios en t_c2b0a22f).

En Windows se guarda en el **Administrador de credenciales** con `wincred`
(`CredWriteW` / `CredReadW` / `CredDeleteW`, `CRED_TYPE_GENERIC`,
`CRED_PERSIST_LOCAL_MACHINE`), con un nombre fijo:

```
WhisprOpen/llm-api-key
```

- Al cargar la configuración, si el campo en memoria está vacío se lee del
  Administrador de credenciales (`Config::load()`); al guardar, se escribe (o se
  borra si lo has dejado vacío).
- Migración: cualquier valor antiguo que estuviera en el config.json de versiones
  previas se conserva para volver a guardarlo en el Administrador de credenciales
  la primera vez que se guarde.
- La pista que hay bajo el campo API key en la app lo dice claro: «Se guarda en
  el Administrador de credenciales de Windows, nunca en la config».

El campo solo aparece en los proveedores que pueden pedirla (Custom y Cloud); con
Local no se muestra (el servidor local no usa ese encabezado: llama.cpp lo
ignora).

---

## 4. Ciclo de vida del corrector (memoria RAM/VRAM)

Requisito t_301476a8: **si la corrección con IA está apagada, el servidor y el
modelo no ocupan memoria; si está encendida, se cargan**. El «modelo» es el
proceso `llama-server.exe` que sirve la IA: descargarlo es terminar ese proceso.

### Arranque (`app.rs::run_startup_thread` → `run_llm_phase`)

1. Whisper: descarga el modelo GGML si falta (con progreso a la vista) y lo
   **deja cargado** (`transcribe::preload`) para que la primera pulsación del
   atajo no pague la carga.
2. Fase de IA solo si `ai_correction_enabled()`:
   - endpoint remoto → petición de prueba con `server_alive_auth` (ver §2);
   - endpoint local → servidor ya vivo, o arrancarlo y esperar 60 s (ver §5).
3. El aviso de estado recibe `LlmReady { launched }` / `LlmDisabled(reason)` /
   `LlmStarting` / `LlmDisabled("")` (se omite en silencio si el interruptor está
   apagado).

### Apagar en caliente (`Message::ToggleAiCorrection(false)` → `disable_ai_correction_now`)

Orden exacto, dejando el estado coherente:

1. **Corrección en marcha** (`llm_rx`/`llm_pending`) → se queda en **texto tal
   cual** (`restore_pending_as_dictation`). Nunca se pierde el turno.
2. **Diff esperando confirmación** → se descarta (el texto original ya está en la
   nota).
3. **Fase de IA en marcha** (hilo de arranque) → se pone el token de cancelación
   a `true`: el hilo se para en su siguiente punto de control (sin arrancar el
   servidor, o terminando el proceso que él mismo lanzó).
4. **Descargar el modelo**: `llm::kill_launched_server()` termina **solo el
   proceso que registró esta app**. Un servidor ajeno (que ya corría antes de
   abrir la app) NO se toca.
5. Aviso a la vista: «Corrección IA desactivada — modelo LLM descargado de
   memoria ✓» si se cerró el proceso propio.

### Encender en caliente (`ToggleAiCorrection(true)`)

`spawn_llm_startup()` relanza la fase de IA (comprobar o arrancar llama-server y
esperar a `/v1/models`). Si el servidor no levanta, `LlmDisabled` enseña el
motivo. Se evita duplicar una fase que ya va bien; una fase cancelada se
reemplaza sin esperar a que muera el hilo viejo (su token lo para).

### Salir de la app

Al salir (`kill_launched_server` en el cierre) se termina el servidor que lanzó
esta instancia, igual que al apagar el interruptor. No queda ningún
`llama-server.exe` suelto (fallo t_9fbc886f).

### Regla del PID propio

`llm::LAUNCHED_SERVER_PID` (AtomicU32) guarda únicamente el proceso creado por
`launch_server`. `terminate_server_pid` comprueba que ese proceso siga siendo el
del servidor antes de matarlo. Nunca se termina un proceso que la app no haya
creado.

---

## 5. Server LLM local nativo (llama.cpp)

La app arranca `llama-server.exe` como **proceso nativo de Windows** (se
descartó intentarlo con WSL: un servidor de Linux lanzado desde la app se quedaba
suelto al salir, fallo t_9fbc886f). Detalles comprobados con el binario real:

- **Dónde está el ejecutable** — `llm::resolve_server_path(cfg)`:
  1. La ruta escrita a mano en el campo **Server LLM** de Ajustes (clave
     `llama_server_path` en config.json), si existe en disco;
  2. si no, búsqueda automática en este orden: `D:\LlamaCPP\llama-server.exe` /
     `C:\LlamaCPP\llama-server.exe` → junto al exe de la app (distribución
     portable) → `%LOCALAPPDATA%\LlamaCPP\llama-server.exe` → PATH de Windows.
- **Dónde está el modelo** — `llm::resolve_model_path(model)` (`ornith-1.0-9b`
  por defecto): si el nombre es una ruta que existe se usa tal cual; si no, se
  busca por nombre base en LM Studio (`~/.lmstudio/models`, incluida
  `lmstudio-community`), `D:/LlamaCPP/models`, `C:/LlamaCPP/models` y
  `D:/models` / `C:/models`, bajando como mucho 6 niveles.
- **Arranque** (`llm::launch_server`, solo Windows):
  - `current_dir` = la carpeta del exe (el binario es un lanzador que carga
    `llama-server-impl.dll` + `cublas64_13.dll`/`ggml-*` desde ahí; con otra
    carpeta falla con STATUS_ENTRYPOINT_NOT_FOUND);
  - `CREATE_NO_WINDOW` (servidor sin ventana de consola);
  - stdout/stderr → `%APPDATA%\whispr-open\llama-server.log` (para ver por qué
    falló un arranque);
  - argumentos: `-m <modelo> --port <puerto>` (el puerto sale de
    `port_from_base_url`, por defecto 8081).
- **Espera de carga** — `llm::wait_for_server(base_url, 60 s)`: pregunta cada
  400 ms a `/v1/models`. Un 0.8B Q8 carga en ~2-5 s en GPU; 60 s dan margen para
  modelos grandes. Si se agota, `LlmDisabled(razón)` y el dictado sigue.

El campo **Server LLM** de Ajustes (`Message::LlmServerPathChanged`) tiene un
texto de ejemplo informativo (`auto_server_placeholder`: la ruta que se
resolvería en ese momento, o «auto: no encontrado — usa Examinar… o un
endpoint») y botones «Examinar…» / «✕» (volver a la búsqueda automática). La
pista de la app: «Vacío = autodetección (D:\LlamaCPP, junto a la app…). Solo
indícalo aquí si la app no lo encuentra sola.»

Sin ejecutable y sin otro endpoint configurado, la corrección se desactiva con un
aviso y el dictado sigue funcionando (configúralo en Ajustes o usa un proveedor
Custom/Cloud).

---

## 6. Modo IN-TURN (principal desde 10/09/2026)

### 6.1 Motivación

Lo más natural al dictar seguido es **corregirte sobre la marcha**, en el mismo
chorro de voz:

> «que calor que hace en murcia **digo cartagena**»

Sin el modo in-turn, la app escribiría tal cual «que calor que hace en murcia
digo cartagena», con las palabras que usaste para corregirte dentro, y esas NO
deben aparecer en el texto final.

### 6.2 Detección

`src/domain/corrective.rs::classify_in_turn(turn) -> bool` (+ la variante con
interruptor `classify_in_turn_with(turn, enabled)` para apagarlo en el futuro).
Reconoce las señales típicas de auto-corrección:

- `digo X` / `diga X` / `dime X`
- `quiero decir X` / `quise decir X` / `mejor dicho X`
- `en vez de` / `en lugar de` (en contexto)
- `mejor X que Y` / `no X sino Y`

A diferencia de `classify` (§7), **no exige un numeral ni una referencia a lo
anterior**: el «antes» y el «después» están en el mismo turno. Filtros de
seguridad: turno de 6 caracteres o más, 3 o más antes de la señal y 1 o más
después (así se evitan cosas como «digo sí», «digo»…).

### 6.3 Redacción: el LLM devuelve el texto final, no un diff

`src/llm.rs::redact(turn, cfg) -> Result<String, String>`. Prompt
`SYSTEM_PROMPT_IN_TURN` (el texto entero está en `src/llm.rs`): el modelo debe
(1) detectar la auto-corrección, (2) cambiar lo de antes de la señal por lo de
después, (3) borrar TODAS las palabras usadas para corregirse, (4) devolver el
texto tal cual si no hay corrección. Devuelve SOLO texto, sin JSON ni bloques de
código.

`clean_redact_output` (limpia la respuesta): quita bloques markdown, prefijos
tipo «Output:»/«Resultado:», comillas de alrededor (rectas, tipográficas, «»…) y
quita espacios sobrantes.

### 6.4 Guard anti-truncación (t_6b9348de, obligatorio)

Problema real: el modelo local pequeño (Qwen3.5-0.8B) contestaba a turnos largos
**sin** corrección con un RESUMEN (registro 01:34:54: entrada de 73 palabras →
salida de 13). Se probaron 9 variantes de prompt contra el servidor real con la
petición exacta y ninguna consigue a la vez respetar los turnos largos y mantener
las correcciones de siempre (reforzar «no resumas» rompía auto-correcciones: «el
cliente es Acme quiero decir Microsoft» devolvía «el cliente es Acme»). Por eso el
prompt se queda como está y la protección es responsabilidad OBLIGATORIA del
código:

`src/llm.rs::guard_anti_truncacion(input, output) -> String`:

- ratio = palabras(output) / palabras(input) (`word_count` por espacios).
- Si `ratio >= 0.6` → se acepta la salida.
- Si hay una **señal inequívoca** de auto-corrección
  (`has_unambiguous_in_turn_marker`: frases como «quiero decir», «mejor dicho»,
  «en vez de», «en lugar de», «rectifico», «corrijo», o las palabras sueltas
  «digo»/«sino» como palabra completa) y `ratio >= 0.3` → se acepta (una
  corrección de verdad quita un trozo + las palabras de la corrección: «no voy a
  las tres sino a las cuatro» → «voy a las cuatro» recorta más de un 40 % con
  razón).
- En cualquier otro caso → **se descarta la salida y se devuelve la entrada
  original** (se escribe lo que dijiste, con tus palabras de corrección si las
  había: nunca se pierde texto).

En `app.rs`, después de `redact`: respuesta vacía → texto tal cual; si `guard`
devuelve la entrada → aviso «La corrección IA devolvió un resumen del turno; se
mantiene el texto original» y se escribe la entrada; si aún quedan palabras de
corrección (`contains_in_turn_marker`) → texto tal cual.

### 6.5 Guard numérico determinista (t_1fc864ef, ampliado en t_d64797c2)

`src/llm.rs::guard_numeric_substitution(input, output) -> Result<String, ()>` es
la red de seguridad del CÓDIGO para las auto-correcciones con **números** («3
digo 4», «tres, digo, cuatro»), donde el modelo local pequeño (Qwen3.5-0.8B-Q8)
no es fiable. En la ENTRADA se busca el patrón inequívoco «NUM_A [texto]
[señal] NUM_B» y, si la salida de la IA no lo resolvió, se rehace el cambio a
mano: `input[..NUM_A] + NUM_B + input[NUM_B..]` (desaparecen NUM_A, la señal y
las palabras de corrección que hay entre los dos).

- Señales reconocidas: `digo`, `diga`, `dime`, `quiero decir`, `quise decir`,
  `mejor dicho`, `en vez de`, `en lugar de` y la construcción `no X sino Y`. La
  expresión regular vive en `NUMERIC_MARKER_REGEX` (dentro de `llm.rs`, más
  estricta que las señales del clasificador `IN_TURN_MARKERS`).
- Números: dígitos y una lista corta de palabras (`NUM_WORDS`: uno–quince,
  veinte–noventa, cien, mil). No es un lector de números universal.
- **NUM_A es el último NÚMERO del trozo anterior a la señal**, no la última
  palabra: al hablar normal el número lleva detrás un sustantivo y/o un verbo
  («me encontré a **tres personas**, digo **cuatro personas**»), y exigir la
  última palabra dejaba fuera esos casos. Lo que ahora NO se tolera es la duda:
  si el trozo anterior tiene 2 o más números, no se puede saber cuál corriges y
  la protección **no toca nada** (mejor no acertar que estropear).
- La protección NO interviene (devuelve `Err(())`) si la IA ya acertó (su salida
  tiene NUM_B y no NUM_A) ni cuando no hay patrón inequívoco. En esos casos quien
  llama acepta la salida de la IA tal cual.
- Cuando SÍ interviene, el texto rehecho es el que se aplica: en
  `handle_llm_redaction` (app.rs) la reconstrucción es `effective_clean_text`, se
  pasa por la protección contra respuestas recortadas (§6.4) y es lo que se
  escribe.

Casos cubiertos por tests (los casos reales del registro, §H1 de
`QA-REPORT-DEPURACION.md`):

| Log | Input del turno | Output del LLM (erróneo) | Inyectado |
|---|---|---|---|
| 14/09 03:19 | «…me encontré a **tres personas, digo cuatro personas**, que iban por la acera…» | «…me encontré a **tres** personas que iban por la acera…» (numeral VIEJO, meta-lengua borrada) | «…me encontré a **cuatro** personas, que iban por la acera…» |
| 13/09 01:37 | «…me encontré a **23** personas que iban, **digo**, a **24** personas que iban…» | «…me encontré a **23** personas que iban a **24** personas…» (concatenación) | «…me encontré a **24** personas que iban por la calle…» |

### 6.6 Caso canónico

| Paso | Valor |
|---|---|
| Input (turno completo) | «que calor que hace en murcia digo cartagena» |
| `classify_in_turn` | true |
| LLM input | el turno completo |
| LLM output | «que calor que hace en cartagena» |
| Guard | ratio 4/7 ≥ 0.6 → acepta |
| Inyectado en app activa | «que calor que hace en cartagena» |

---

## 7. Modo CROSS-TURN (histórico, fallback)

### 7.1 Detección

`corrective::classify` / `classify_with(turn, threshold)` — un clasificador
barato por reglas (señales por orden de peso): negaciones al principio («no»,
«así no», «mal», «me equivoqué»), números + «en vez de»/«en lugar de»/«no…sino»,
referencias al texto anterior («lo último», «eso», «ese número», «la última
frase»), verbos de corrección («corrige», «cambia», «pon», «quita», «borra»,
«sustituye»).

Dos niveles: **A (seguro, sin IA)** = encaja con los `CORRECTION_PATTERNS` de
`correction.rs` → se aplica por reglas; **B (necesita IA)** = negación o verbo de
corrección Y un número o una referencia a lo anterior, con la sensibilidad que
marque `threshold` (config `llm_classifier_threshold`, por defecto 50, rango
0..=100; el clasificador traduce el número de señales a {50, 75, 100} — 50
permisivo, 100 no manda nunca a la IA, 0 pasivo). La duda se resuelve a favor del
dictado normal (si la app cree que te corriges y no era eso, borra texto: mejor
no acertar que estropear).

### 7.2 Contrato de diff

`src/llm.rs::correct(target, instruction, cfg) -> Result<Diff, String>`, prompt
`SYSTEM_PROMPT` (el texto entero está en `src/llm.rs`). El modelo devuelve SOLO
un JSON válido:

```json
{"find": "<subcadena exacta en TEXTO ACTUAL>", "replace": "<nuevo texto>"}
```

Reglas del prompt: `find` es un trozo literal (palabra, número o frase corta; si
no lo encuentra, `{"find":"","replace":""}`); `replace` vacío si se pide quitar;
no inventar texto; **resolver los números** («2 en vez de 4» sobre «…4…» → find
«4», replace «2») y las **referencias** («lo último» → la última frase o
número).

`parse_diff` saca el JSON (con `last_balanced_json_object` para respuestas que
traen texto alrededor) y `extract_message_text` lee
`choices[0].message.content`, y si viene vacío mira `reasoning_content` (modelos
que razonan; la petición manda `chat_template_kwargs.enable_thinking=false` y
`max_tokens` 256).

### 7.3 Aplicar el diff

En `app.rs` (resultado del hilo → `Message::CorrectionDone`):

1. Comprobar que `find` está literalmente en `target` (`corrective::apply_diff`:
   si no está → se descarta y la instrucción se trata como dictado normal; no se
   rompe el texto).
2. Sustituir **la primera aparición** de `find` por `replace`.
3. Apuntarlo en el historial de deshacer (`{before, after}`) — 1 nivel, botón
   «Deshacer» (`Message::UndoLastCorrection`: «Corrección IA deshecha ↩»).
4. Actualizar `transcript` y `last_turn` con el texto corregido.

Ejemplo de siempre: «no no lo último está mal, es 2 en vez de 4» sobre «la
reunión es a las 4 en punto» → «la reunión es a las 2 en punto».

---

## 8. Fallback por reglas (sin LLM)

`src/domain/correction.rs::apply_correction(input) -> CorrectionResult` +
`CorrectionSession` para varios turnos. Casos fijos:

- Número + «en vez de» / «en lugar de»:
  `no[^,]*lo último[^,]*es (\d+) en vez de (\d+)` → sustituir `$2` por `$1` en
  `last_turn`.
- «era X no Y» / «es X no Y» con números: `(?:era|es)\s+(\d+)\s+no\s+(\d+)`.
- Referencia explícita a «la última palabra» → sustituir la última palabra.
- Reformulaciones completas: «no, quería decir X», «mejor dicho X», «me refería
  a X», «donde dije X, quería decir Y».

Las reglas y la IA trabajan con el mismo contrato. Cuando el clasificador ve
intención de corregir que las reglas no resuelven, el turno se manda al
proveedor configurado (§2). Solo se llama a la IA cuando hay señales de
corrección: el dictado normal no paga ese coste.

---

## 9. UX: transparencia y opciones claras (t_e25f8ab7)

### 9.1 Chip de estado del corrector — SIEMPRE visible

En la ventana principal (bajo la cabecera) y repetido como línea de estado en
Ajustes. La lógica es una función pura en
`app.rs::llm_status_ui(enabled, status, model)`, con tests:

| Estado | Chip | Color |
|---|---|---|
| Toggle apagado | `○ Corrección IA: apagada — actívala en Ajustes` | gris (dim) |
| `LlmStatus::Ready` | `● Corrección IA: activa · <modelo>` | verde (ok) |
| `LlmStatus::Disabled(motivo)` | `○ Corrección IA: inactiva — <motivo>` | ámbar (warn) |
| `Unknown`/arranque | `○ Corrección IA: comprobando…` | gris |

El motivo de que esté inactiva siempre se ve (nunca en silencio): «el endpoint
https://… no respondió o rechazó la petición; el dictado sigue funcionando sin
corrección» (`remote_endpoint_reason`), «no se encontró el ejecutable del server
LLM (llama-server.exe). Configúralo en Ajustes o usa un endpoint
OpenAI-compatible…», etc.

### 9.2 Ajustes → Corrección IA (primer nivel)

- Línea de estado en vivo (la misma lógica que el aviso).
- Casilla «Corrección IA: una IA revisa y corrige lo que dictas» + explicación:
  «Al terminar un turno, la IA lo revisa y corrige («digo X», «no, era Y»…). Si
  el servidor de IA no está disponible, el dictado se inserta entero, sin
  corregir.»
- **Proveedor** (pick_list con las etiquetas de §2) + explicación: «Quién corrige
  lo que dictas: Local — llama.cpp, el que viene con la app (sin internet,
  recomendado) · Custom LLM — tu propio endpoint OpenAI-compatible · Cloud —
  OpenAI. Por defecto: Local.»
- **Endpoint LLM** (text input) + «Dirección del servidor de IA (API compatible
  con OpenAI). Local: http://127.0.0.1:8081/v1.»
- **Modelo IA** (text input, placeholder «ornith-1.0-9b / gpt-4o-mini») + «El
  modelo que corrige. Debe estar en tu servidor local o ser uno de los que
  ofrece la nube.»
- **API key** (solo Custom/Cloud, `secure(true)`) + «Clave del endpoint si la
  exige (Custom: opcional · Cloud: obligatoria). Se guarda en el Administrador
  de credenciales de Windows, nunca en la config.»
- **Server LLM** (ruta `llama-server.exe` + Examinar…/✕) + placeholder
  autodetección + la pista de §5.

### 9.3 Ajustes avanzados (técnico, colapsado por defecto)

Cada opción lleva su explicación en lenguaje llano (el texto es el mismo que se
ve en la app):

- **Calidad de transcripción (beam size)** — slider 1..=8, default 5: «Calidad
  de transcripción (beam size) vs velocidad: 1 = la más rápida, 8 = la más
  fiel. Rango 1–8, recomendado: 5.»
- **Núcleos de CPU** — slider 1..=16, default 4: «Núcleos de CPU que usa Whisper
  al transcribir. Sube si el equipo va justo de velocidad; baja para dejar
  recursos al resto. Rango 1–16, recomendado: 4.»
- **Espera máx. (s)** — tiempo máximo de la IA, slider 1..=120, default 25:
  «Espera máxima de la IA antes de dar el turno por definitivo: si lo supera, se
  inserta sin corregir. Rango 1–120 s, recomendado: 25 s.»
- **Sensibilidad** — umbral del clasificador 0..=100, default 50: subirlo hace la
  app más prudente (menos llamadas a la IA, más dictado normal por defecto).

### 9.4 Corrección consciente

Mientras la IA trabaja, `status = Status::Correcting` (ámbar, como
Transcribing): así sabes que la app «te ha oído» y está en ello. Por defecto se
aplica sola + aviso en el panel de respuesta (`last_correction`) + deshacer de 1
nivel. Hay un interruptor opcional **«Confirmar antes de aplicar»**
(`ai_correction_confirm`, apagado por defecto): entonces el diff llega con
botones «Aplicar / Descartar».

---

## 10. Latencia esperada

| Etapa | Coste estimado |
|---|---|
| Whisper Small (RTX 4070, CUDA) | tiempo real / ligeramente inferior (RTF ~0.2–0.5) |
| Clasificadores (regex) | < 1 ms |
| LLM 9B local, prompt ~250–400 tok, ~75 tok/s | ~4–6 s |
| Guard + aplicación + re-render | < 1 ms |

La corrección con IA añade **~4–6 s solo cuando se detecta que quieres
corregir** (in-turn o cross-turn); el dictado normal no lo paga. La espera por
defecto (25 s) cubre ese margen de sobra, también con endpoints remotos.

---

## 11. Mapa de código (nombres reales, verificados)

| Archivo | Símbolos | Papel |
|---|---|---|
| `src/config.rs` | `CorrectionProvider` (+`label`), `Config::correction_provider()`, `Config::llm_settings()`, `llm_api_key` (skip_serializing), `API_KEY_CREDENTIAL_TARGET = "WhisprOpen/llm-api-key"`, `load/save_api_key_secret` (wincred) | persistencia, defaults por proveedor, API key segura |
| `src/llm.rs` | `LlmConfig`, `correct`, `redact`, `clean_redact_output`, `guard_anti_truncacion`, `has_unambiguous_in_turn_marker`, `guard_numeric_substitution`, `parse_numeric_correction`, `is_numeric_token`, `count_numeric_tokens`, `parse_diff`, `extract_message_text`, `server_alive(_auth)`, `is_local_endpoint`, `port_from_base_url`, `launch_server`, `resolve_server_path`, `resolve_model_path`, `wait_for_server`, `LAUNCHED_SERVER_PID`, `kill_launched_server` | cliente OpenAI-compatible (ureq), prompts, guards (anti-truncación y numérico), ciclo de vida del server nativo |
| `src/domain/corrective.rs` | `classify`, `classify_with`, `classify_in_turn(_with)`, `contains_in_turn_marker`, `strip_in_turn_meta`, `apply_diff`, `replace_last_turn`, `Diff`, `CorrectiveDecision` | clasificadores puros (sin I/O) + aplicación de diff |
| `src/domain/correction.rs` | `apply_correction`, `CorrectionSession`, `append_transcript`, `replace_last_sentence` | corrección por reglas (fallback barato) |
| `src/app.rs` | `Message::TranscriptionDone` (routing en cascada), `ToggleAiCorrection`, `CorrectionProviderChanged`, `spawn_llm_startup`, `run_startup_thread`, `run_llm_phase`, `disable_ai_correction_now`, `restore_pending_as_dictation`, `llm_status_ui`, `remote_endpoint_reason`, `auto_server_placeholder`, `LlmStatus`, `Status::Correcting`, botones Undo/Deshacer | UI, máquina de estados, hilos LLM (std::thread + mpsc), chip de estado |

Configuración de arranque (no UI): `%APPDATA%\whispr-open\config.json` — ver
`README.md` → Configuración avanzada y `docs/SETUP.md` para Local/Custom/Cloud.

---

## 12. Criterios de aceptación (cómo se verifica)

- `cargo test` verde (**336 passed + 3 ignored** a 24/09/2026, incluidos los que
  añadió t_c2b0a22f sobre el selector de proveedor y el degradado a remoto).
  Cubren: clasificadores cross-turn e in-turn (el caso de siempre del autor «que
  calor en murcia digo cartagena» + variantes), `apply_diff` (números, «lo
  último», diff inválido que no toca nada), deshacer, `clean_redact_output`,
  `contains_in_turn_marker` / `strip_in_turn_meta`, protección contra respuestas
  recortadas (casos largo y corto), protección numérica (§6.5: concatenación,
  número viejo en frase natural —los dos casos del registro—, borrado de
  números, `no…sino`, la duda que NO se toca y un caso de control sin
  corrección), estado del aviso, etiquetas de proveedor, valores por defecto de
  `llm_settings` según proveedor.
- `cargo check` sin errores.
- Casos de siempre: cross-turn «la reunión es a las 4 en punto» + «no no lo
  último está mal, es 2 en vez de 4» → «la reunión es a las 2 en punto»;
  in-turn «que calor que hace en murcia digo cartagena» → «que calor que hace
  en cartagena».
- Endpoint remoto caído → `LlmDisabled` con un aviso que nombra la URL y deja
  claro que el dictado sigue (test `endpoint_remoto_caido_desactiva_con_aviso_
  que_no_bloquea`).
- Toggle off → ningún `llama-server.exe` en RAM/VRAM (solo el proceso propio);
  Salir → lo mismo.

## 13. Decisiones cerradas

- Motor: servidor **compatible con OpenAI por ureq** (sin meter un motor nuevo
  dentro de la app); la IA corre en un hilo (`std::thread::spawn` + canal mpsc,
  nunca en el bucle de la interfaz ni en el hilo del audio).
- Alcance del diff cross-turn: `last_turn`; la nota se actualiza solo si el
  texto a corregir sigue estando.
- In-turn devuelve **el texto final ya escrito**, no un diff (el contenido del
  turno es a la vez la entrada y la salida).
- Protección contra respuestas recortadas **en el código, no en el prompt** (9
  variantes probadas contra el servidor real; el prompt sin refuerzos respeta
  las correcciones).
- Protección numérica que antepone **no equivocarse a acertar siempre**: si el
  trozo anterior a la señal tiene 2 o más números, no se puede saber cuál
  corriges y no se toca nada (lo peor que puede pasar es escribir la salida de
  la IA o el texto tal cual, nunca inventar una cifra). Cuando sí decide, su
  reconstrucción es el texto que se aplica y se escribe.
- Confirmación: apagada por defecto (se aplica + aviso + deshacer);
  `ai_correction_confirm` la activa desde Ajustes.
- API key exclusivamente en el Administrador de credenciales (Windows),
  `skip_serializing` en el JSON, y migración de lo antiguo en el primer guardado.
- El servidor `llama-server.exe` se arranca y se termina como proceso nativo con
  su proceso registrado; nunca se mata un servidor ajeno.
