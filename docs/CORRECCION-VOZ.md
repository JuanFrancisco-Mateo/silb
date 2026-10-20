# CORRECCIÓN / FORMATEO POR VOZ — especificación y diseño final

Versión de diseño: **10/09/2026** — consolidada tras t_9fbc886f (server LLM
nativo y ruta configurable), t_6b9348de (guard anti-truncación), t_301476a8
(ciclo de vida de memoria), t_e25f8ab7 (UX de opciones y transparencia),
t_c2b0a22f (proveedores Local/Custom/Cloud), t_1fc864ef (guard numérico
determinista) y t_d64797c2 (ampliación de ese guard a frases naturales + la
reconstrucción como texto realmente inyectado). Este documento es la fuente de
verdad de **cómo funciona hoy** el corrector IA; los nombres de archivos y
funciones que se citan están verificados contra el código (rama `native-iced`).

Objetivo: que el usuario pueda corregir lo que acaba de dictar **hablando**,
con frases como el ejemplo canónico:

> «no no lo último está mal, es 2 en vez de 4»

y que la app entienda la intención correctiva, localice el fragmento a cambiar
y lo corrija en el texto final.

Es **post-procesado del texto transcrito**: no toca la captura de audio (cpal)
ni la decodificación de Whisper. Cualquier fallo del corrector degrada a texto
bruto — **el dictado nunca se bloquea ni pierde texto**.

---

## 1. Arquitectura: cascada de routing

En `app.rs`, `Message::TranscriptionDone` decide qué hacer con el turno recién
transcrito (`processed`) en cascada, de más prioritario a menos:

1. **Reglas** (`src/domain/correction.rs`, `apply_correction` +
   `CorrectionSession`): si el turno casa con un patrón determinista
   («no, quería decir X», «mejor dicho X», numeral + «en vez de»…), se aplica
   sin LLM y el flujo termina aquí (coste ~0).
2. **IN-TURN** (modo principal): si `classify_in_turn(processed)` → el LLM
   recibe el **turno completo** y devuelve el **texto final corregido** (sin
   meta-lengua). No exige turno previo: la corrección vive dentro del mismo
   chorro de voz («que calor en murcia **digo cartagena**»).
3. **CROSS-TURN** (modo histórico, fallback): si
   `corrective::classify_with(processed, threshold) == NeedsLlm` y existe
   `last_turn` previo → el LLM recibe `(last_turn, instruction)` (el target y
   la frase correctiva) y devuelve un diff JSON `{find, replace}`.
4. **PLAIN**: el resto → dictado normal sin intervención del LLM.

Gates comunes a los dos modos LLM (en `TranscriptionDone`):

```rust
let can_route_common = !rule_corrected && enabled && !llm_busy && llm_ready;
// enabled  = config.ai_correction_enabled()
// llm_ready = matches!(self.llm_status, LlmStatus::Ready { .. })
// llm_busy = self.llm_rx.is_some() || self.llm_pending.is_some()
```

Si un turno dispara AMBOS clasificadores (caso mixto, p. ej. «no no lo último
está mal es 2 en vez de 4», que tiene referencia previa Y «en vez de»), gana el
**in-turn** por prioridad.

El log del archivo `%APPDATA%\whispr-open\whispr-open.log` registra el
resultado de cada clasificador, los gates que cortan y la decisión de enrutar
(sin consola, el `println!` viejo era invisible — bug 09/09/2026).

---

## 2. Proveedores: Local / Remoto (OpenAI) / Remoto (endpoint propio)

Selector `pick_list` en el primer nivel de Ajustes → Corrección IA
(`Message::CorrectionProviderChanged`). Enum en `src/config.rs`:

```rust
pub enum CorrectionProvider { Default, Cloud, Custom }
// serde rename_all = "lowercase"  →  "default" | "cloud" | "custom" en config.json
```

Etiquetas del selector (`CorrectionProvider::label()`, sprint final): el
usuario solo ve dos modos, Local o Remoto.

| Proveedor | Etiqueta en la UI | API key |
|---|---|---|
| `Default` | Local (en este equipo) | nunca se muestra |
| `Cloud` | Remoto (OpenAI) | obligatoria |
| `Custom` | Remoto (endpoint propio) | opcional |

**Default = Local** (`is_local()`): `correction_provider()` devuelve `Default`
ante `None` (configs antiguas sin el campo no cambian de comportamiento).

`Config::llm_settings()` monta la configuración efectiva del corrector
(`llm::LlmConfig`) con defaults por proveedor, todos overridables por campos
individuales:

| Proveedor | `base_url` default | `model` default | key |
|---|---|---|---|
| Default (Local) | `http://127.0.0.1:8081/v1` | `ornith-1.0-9b` | — |
| Cloud | `https://api.openai.com/v1` | `gpt-4o-mini` | sí |
| Custom | `http://127.0.0.1:8081/v1` | `ornith-1.0-9b` | si el server la pide |

Los campos `llm_base_url` y `llm_model` se comparten entre proveedores: si el
usuario pone un endpoint propio, Custom (o Cloud) lo usa tal cual; vacíos caen
al default del proveedor activo. Los valores se triman y un campo vacío se
trata como «no escrito» (None → default), nunca como cadena inválida.

### Comprobación del endpoint en arranque / cambio de proveedor

En `app.rs::run_llm_phase` (fase de arranque o relanzada al cambiar de
proveedor):

- **Endpoint remoto** (Custom/Cloud con host no-loopback, `llm::is_local_endpoint`
  = false): **no hay server local que levantar**. Se hace un probe
  `GET {base_url}/models` con timeout de 2 s (`llm::server_alive_auth`),
  mandando `Authorization: Bearer <key>` **solo si hay key** (sin ella, un 401
  falsearía «caído»). Si responde → `LlmReady { launched: false }`; si no →
  `LlmDisabled(remote_endpoint_reason(url))` con aviso visible.
- **Endpoint local**: si `/v1/models` ya responde → `LlmReady`; si no, se
  resuelve y lanza `llama-server.exe` (ver §5) y se espera a que el modelo
  cargue (máx. 60 s). Fallo → `LlmDisabled(motivo)`.

`server_alive*` usa `/v1/models` y no `/health`: llama.cpp responde 503 en
`/health` mientras carga el GGUF, pero `/models` solo contesta cuando el modelo
está listo — es la señal fiable de «server vivo y modelo cargado».

Al cambiar de proveedor (`CorrectionProviderChanged`), una fase en vuelo con la
config antigua se cancela (token) y se relanza `spawn_llm_startup()` para que el
chip de estado refleje el proveedor activo. Cualquier fallo termina en
`LlmDisabled(razón)`: la corrección se desactiva con aviso visible y el dictado
sigue a texto bruto.

---

## 3. API key: Credential Manager, NUNCA config.json

El campo `llm_api_key` de `Config` lleva `#[serde(skip_serializing)]`: **no se
escribe en `%APPDATA%\whispr-open\config.json` en ningún caso** (persistencia
desde el diseño original, sin cambios en t_c2b0a22f).

En Windows se guarda en el **Administrador de credenciales** con
`wincred` (`CredWriteW` / `CredReadW` / `CredDeleteW`, `CRED_TYPE_GENERIC`,
`CRED_PERSIST_LOCAL_MACHINE`), target fijo:

```
WhisprOpen/llm-api-key
```

- Al cargar la config, si el campo en memoria está vacío se lee del Credential
  Manager (`Config::load()`); al guardar, se escribe (o borra si el usuario la
  vació).
- Migración: cualquier valor legado que existiera en config.json de versiones
  antiguas se conserva para reescribirlo al Credential Manager en el primer
  guardado.
- El hint bajo el campo API key en la UI lo dice explícitamente: «Se guarda en
  el Administrador de credenciales de Windows, nunca en la config».

El campo solo se muestra para proveedores que pueden exigirla (Custom y Cloud);
con Local no aparece (el server local no usa Bearer: llama.cpp la ignora).

---

## 4. Ciclo de vida del corrector (memoria RAM/VRAM)

Requisito t_301476a8: **toggle de corrección IA apagado = server y modelo fuera
de memoria; encendido = carga**. El «modelo» es el proceso `llama-server.exe`
que sirve el LLM: descargarlo = terminar el proceso.

### Arranque (`app.rs::run_startup_thread` → `run_llm_phase`)

1. Whisper: descarga el modelo GGML si falta (progreso visible) y lo **precarga**
   (`transcribe::preload`) para que la primera pulsación del atajo no pague la
   carga.
2. Fase LLM solo si `ai_correction_enabled()`:
   - endpoint remoto → probe `server_alive_auth` (ver §2);
   - endpoint local → server ya vivo, o lanzar + esperar 60 s (ver §5).
3. El chip de estado recibe `LlmReady { launched }` / `LlmDisabled(reason)` /
   `LlmStarting` / `LlmDisabled("")` (omisión silenciosa si el toggle está off).

### Apagar en caliente (`Message::ToggleAiCorrection(false)` → `disable_ai_correction_now`)

Orden exacto, dejando la máquina de estados coherente:

1. **Corrección en vuelo** (`llm_rx`/`llm_pending`) → degrada a **texto bruto**
   (`restore_pending_as_dictation`). Nunca se pierde el turno del usuario.
2. **Diff pendiente de confirmación** → descartado (el texto bruto del target ya
   está en el transcript).
3. **Fase LLM en vuelo** (hilo de arranque) → token de cancelación a `true`: el
   hilo aborta en su siguiente punto de control (sin lanzar server, o
   terminando el PID que él mismo lanzó).
4. **Descargar el modelo**: `llm::kill_launched_server()` termina **solo el PID
   que registró esta app**. Un server ajeno (ya corría antes de abrir la app) NO
   se toca.
5. Feedback visible: «Corrección IA desactivada — modelo LLM descargado de
   memoria ✓» si se mató el proceso propio.

### Encender en caliente (`ToggleAiCorrection(true)`)

`spawn_llm_startup()` relanza la fase LLM (verificar/arrancar llama-server y
esperar `/v1/models`). Si el server no levanta, `LlmDisabled` muestra el motivo.
Se evita duplicar una fase sana en vuelo; una fase cancelada se reemplaza sin
esperar a que el hilo viejo muera (su token la aborta).

### Salir de la app

El handler de Salir (`kill_launched_server` en el teardown) termina el server
que lanzó esta instancia, igual que el toggle off. No queda ningún
`llama-server.exe` huérfano (bug t_9fbc886f).

### Regla del PID propio

`llm::LAUNCHED_SERVER_PID` (AtomicU32) guarda únicamente el PID creado por
`launch_server`. `terminate_server_pid` verifica que el PID siga siendo la
imagen del server antes de matar. Nunca se termina un proceso que la app no
haya creado.

---

## 5. Server LLM local nativo (llama.cpp)

La app lanza `llama-server.exe` como **proceso nativo de Windows** (descartado
el intento WSL: un server ELF lanzado desde la app quedaba huérfano al salir,
bug t_9fbc886f). Detalles verificados con el binario real:

- **Resolución del ejecutable** — `llm::resolve_server_path(cfg)`:
  1. Ruta explícita del campo **Server LLM** de Ajustes (clave
     `llama_server_path` en config.json) si existe en disco;
  2. autodetección en orden: `D:\LlamaCPP\llama-server.exe` /
     `C:\LlamaCPP\llama-server.exe` → junto al exe de la app (distribución
     portable) → `%LOCALAPPDATA%\LlamaCPP\llama-server.exe` → PATH de Windows.
- **Resolución del modelo** — `llm::resolve_model_path(model)` (`ornith-1.0-9b`
  por defecto): si el nombre es una ruta que existe se usa literal; si no, se
  busca por nombre base en LM Studio (`~/.lmstudio/models`, incl.
  `lmstudio-community`), `D:/LlamaCPP/models`, `C:/LlamaCPP/models` y
  `D:/models` / `C:/models`, con recorrido acotado a profundidad 6.
- **Spawn** (`llm::launch_server`, solo Windows):
  - `current_dir` = directorio del exe (el binario es un stub que carga
    `llama-server-impl.dll` + `cublas64_13.dll`/`ggml-*` desde ahí; otro cwd
    falla con STATUS_ENTRYPOINT_NOT_FOUND);
  - `CREATE_NO_WINDOW` (server sin consola);
  - stdout/stderr → `%APPDATA%\whispr-open\llama-server.log` (diagnóstico de
    arranques fallidos);
  - args: `-m <modelo> --port <puerto>` (puerto de `port_from_base_url`, default
    8081).
- **Espera de carga** — `llm::wait_for_server(base_url, 60 s)`: polling cada
  400 ms contra `/v1/models`. Un 0.8B Q8 carga en ~2-5 s en GPU; 60 s cubren
  modelos grandes. Si se agota, `LlmDisabled(razón)` y el dictado sigue.

El campo **Server LLM** de Ajustes (`Message::LlmServerPathChanged`) tiene
placeholder informativo (`auto_server_placeholder`: la ruta que se resolvería
ahora mismo, o «auto: no encontrado — usa Examinar… o un endpoint») y botones
«Examinar…» / «✕» (reset a autodetección). El hint de la UI: «Vacío =
autodetección (D:\LlamaCPP, junto a la app…). Solo indícalo aquí si la app no lo
encuentra sola.»

Sin ejecutable y sin endpoint alternativo configurado, la corrección se
desactiva con aviso y el dictado sigue funcionando (configurar en Ajustes o usar
un proveedor Custom/Cloud).

---

## 6. Modo IN-TURN (principal desde 10/09/2026)

### 6.1 Motivación

El caso más natural al dictar fluidamente es **corregirse sobre la marcha**, en
el mismo chorro de voz:

> «que calor que hace en murcia **digo cartagena**»

Sin soporte in-turn, la app escribiría textualmente
«que calor que hace en murcia digo cartagena» — incluyendo la meta-lengua
correctiva, que NO debe aparecer en el texto final.

### 6.2 Detección

`src/domain/corrective.rs::classify_in_turn(turn) -> bool` (+ variante con
kill-switch `classify_in_turn_with(turn, enabled)` para toggles futuros).
Reconoce los marcadores típicos de auto-corrección:

- `digo X` / `diga X` / `dime X`
- `quiero decir X` / `quise decir X` / `mejor dicho X`
- `en vez de` / `en lugar de` (en contexto)
- `mejor X que Y` / `no X sino Y`

A diferencia de `classify` (§7), **no exige numeral ni referencia anafórica**:
el «antes» y el «después» viven en el mismo turno. Filtros de seguridad: turno
≥ 6 chars, ≥ 3 chars antes del marcador y ≥ 1 char después (evita «digo sí»,
«digo»…).

### 6.3 Redacción: el LLM devuelve el texto final, no un diff

`src/llm.rs::redact(turn, cfg) -> Result<String, String>`. Prompt
`SYSTEM_PROMPT_IN_TURN` (texto íntegro en `src/llm.rs`): el modelo debe
(1) detectar la auto-corrección, (2) sustituir lo anterior al marcador por lo
posterior, (3) eliminar TODA la meta-lengua, (4) devolver el texto tal cual si
no hay corrección. Devuelve SOLO texto, sin JSON ni fences.

`clean_redact_output` (post-proceso de la respuesta): quita fences markdown,
prefijos tipo «Output:»/«Resultado:», comillas envolventes (rectas,
tipográficas, «»…) y hace trim.

### 6.4 Guard anti-truncación (t_6b9348de, obligatorio)

Problema real: el modelo local pequeño (Qwen3.5-0.8B) respondía a turnos largos
**sin** corrección con un RESUMEN (log 01:34:54: input de 73 palabras → output
de 13). Se probaron 9 variantes de prompt en runtime (server real, payload
exacto) y ninguna consigue a la vez preservar los turnos largos y mantener las
correcciones canónicas (reforzar «no resumas» rompía auto-correcciones: «el
cliente es Acme quiero decir Microsoft» devolvía «el cliente es Acme»). Por
eso el prompt se mantiene y la anti-truncación es responsabilidad OBLIGATORIA
del guard de código:

`src/llm.rs::guard_anti_truncacion(input, output) -> String`:

- ratio = palabras(output) / palabras(input) (`word_count` por whitespace).
- Si `ratio >= 0.6` → se acepta el output.
- Si hay **marcador inequívoco** de auto-corrección
  (`has_unambiguous_in_turn_marker`: frases como «quiero decir», «mejor dicho»,
  «en vez de», «en lugar de», «rectifico», «corrijo», o palabras sueltas
  «digo»/«sino» con límite de palabra) y `ratio >= 0.3` → se acepta (una
  corrección real elimina fragmento + meta-lengua: «no voy a las tres sino a
  las cuatro» → «voy a las cuatro» recorta >40% legítimamente).
- En cualquier otro caso → **se descarta el output y se devuelve el input
  original** (se inyecta lo que el usuario dijo, con su meta-lengua si la había:
  nunca se pierde texto).

En `app.rs`, tras `redact`: respuesta vacía → texto bruto; `guard` devuelve el
input → feedback «La corrección IA devolvió un resumen del turno; se mantiene
el texto original» y se inyecta el input; aún contiene meta-lengua
(`contains_in_turn_marker`) → texto bruto.

### 6.5 Guard numérico determinista (t_1fc864ef, ampliado en t_d64797c2)

`src/llm.rs::guard_numeric_substitution(input, output) -> Result<String, ()>`
es la red de seguridad de CÓDIGO para las auto-correcciones con **numerales**
(«3 digo 4», «tres, digo, cuatro»), donde el modelo local pequeño
(Qwen3.5-0.8B-Q8) es poco fiable. En el INPUT se busca el patrón inequívoco
«NUM_A [texto] [marcador] NUM_B» y, si el output del LLM no lo resolvió, se
reconstruye la sustitución deterministamente:
`input[..NUM_A] + NUM_B + input[NUM_B..]` (desaparecen NUM_A, el marcador y la
meta-lengua que va entre ambos).

- Marcadores reconocidos: `digo`, `diga`, `dime`, `quiero decir`,
  `quise decir`, `mejor dicho`, `en vez de`, `en lugar de` y la construcción
  `no X sino Y`. El regex vive en `NUMERIC_MARKER_REGEX` (auto-contenido en
  `llm.rs`, más estricto que el clasificador `IN_TURN_MARKERS`).
- Numerales: dígitos y la lista corta de palabras (`NUM_WORDS`: uno–quince,
  veinte–noventa, cien, mil). No es un parser de números universal.
- **NUM_A es el último NUMERAL del fragmento anterior al marcador**, no el
  último token: en habla natural el numeral lleva detrás sintagma nominal y/o
  verbo («me encontré a **tres personas**, digo **cuatro personas**»), y
  exigir el último token dejaba fuera esos casos. Lo que ahora NO se tolera es
  la ambigüedad: si el fragmento anterior tiene 2+ numerales, no se puede saber
  cuál rectifica el usuario y el guard **no toca nada** (precisión > recall).
- El guard NO interviene (devuelve `Err(())`) si el LLM ya acertó (su output
  contiene NUM_B y no NUM_A) ni cuando no hay patrón inequívoco. En esos casos
  el caller acepta el output del LLM tal cual.
- Cuando SÍ interviene, el texto reconstruido es el que se aplica: en
  `handle_llm_redaction` (app.rs) la reconstrucción es `effective_clean_text`,
  se evalúa contra el guard anti-truncación (§6.4) y es lo que se inyecta.

Cobertura verificada por tests (los casos reales del log del usuario, §H1 de
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

`corrective::classify` / `classify_with(turn, threshold)` — clasificador barato
por reglas (señales en orden de peso): negaciones al inicio («no», «así no»,
«mal», «me equivoqué»), numerales + «en vez de»/«en lugar de»/«no…sino»,
referencias al texto previo («lo último», «eso», «ese número», «la última
frase»), verbos correctivos («corrige», «cambia», «pon», «quita», «borra»,
«sustituye»).

Dos niveles: **A (seguro, sin LLM)** = casa con `CORRECTION_PATTERNS` de
`correction.rs` → se aplica por reglas; **B (requiere LLM)** = negación o verbo
correctivo Y numeral o referencia anafórica, con sensibilidad según `threshold`
(config `llm_classifier_threshold`, default 50, rango 0..=100; el clasificador
mapea el nº de señales a {50, 75, 100} — 50 permisivo, 100 nunca enruta, 0
pasivo). La duda se resuelve hacia dictado normal (un falso positivo de
corrección borra texto; precisión > recall).

### 7.2 Contrato de diff

`src/llm.rs::correct(target, instruction, cfg) -> Result<Diff, String>`,
prompt `SYSTEM_PROMPT` (texto íntegro en `src/llm.rs`). El modelo devuelve SOLO
un JSON válido:

```json
{"find": "<subcadena exacta en TEXTO ACTUAL>", "replace": "<nuevo texto>"}
```

Reglas del prompt: `find` subcadena literal (palabra, número o frase corta; si
no localiza, `{"find":"","replace":""}`); `replace` vacío si se pide quitar;
no inventar texto; **resolver numerales** («2 en vez de 4» sobre «…4…» →
find «4», replace «2») y **referencias** («lo último» → última frase/número).

`parse_diff` extrae el JSON (con `last_balanced_json_object` para respuestas con
texto alrededor) y `extract_message_text` lee `choices[0].message.content`,
cayendo a `reasoning_content` si el content está vacío (modelos de razonamiento;
el body manda `chat_template_kwargs.enable_thinking=false` y `max_tokens` 256).

### 7.3 Aplicar el diff

En `app.rs` (resultado del hilo → `Message::CorrectionDone`):

1. Validar que `find` es subcadena literal de `target` (`corrective::apply_diff`:
   si no lo es → descartar y tratar la instrucción como dictado normal; no
   romper el texto).
2. Sustituir **la primera ocurrencia** de `find` por `replace`.
3. Registrar en el historial de undo (`{before, after}`) — 1 nivel, botón
   «Deshacer» (`Message::UndoLastCorrection`: «Corrección IA deshecha ↩»).
4. Actualizar `transcript` y `last_turn` con el texto corregido.

Ejemplo canónico: «no no lo último está mal, es 2 en vez de 4» sobre «la
reunión es a las 4 en punto» → «la reunión es a las 2 en punto».

---

## 8. Fallback por reglas (sin LLM)

`src/domain/correction.rs::apply_correction(input) -> CorrectionResult` +
`CorrectionSession` multi-turno. Casos deterministas:

- Numeral + «en vez de» / «en lugar de»:
  `no[^,]*lo último[^,]*es (\d+) en vez de (\d+)` → sustituir `$2` por `$1` en
  `last_turn`.
- «era X no Y» / «es X no Y» con numerales: `(?:era|es)\s+(\d+)\s+no\s+(\d+)`.
- Referencia explícita a «la última palabra» → sustituir la última palabra.
- Reformulaciones completas: «no, quería decir X», «mejor dicho X», «me refería
  a X», «donde dije X, quería decir Y».

Las reglas y el LLM comparten el mismo contrato. Cuando el clasificador detecta
intención correctiva que las reglas no resuelven, el turno se envía al proveedor
LLM configurado (§2). El LLM **solo** se invoca en presencia de señales de
corrección: el dictado normal no paga el coste.

---

## 9. UX: transparencia y opciones claras (t_e25f8ab7)

### 9.1 Chip de estado del corrector — SIEMPRE visible

En la ventana principal (bajo el header) y repetido como línea de estado en
Ajustes. Lógica pura en `app.rs::llm_status_ui(enabled, status, model)`,
testeada:

| Estado | Chip | Color |
|---|---|---|
| Toggle apagado | `○ Corrección IA: apagada — actívala en Ajustes` | gris (dim) |
| `LlmStatus::Ready` | `● Corrección IA: activa · <modelo>` | verde (ok) |
| `LlmStatus::Disabled(motivo)` | `○ Corrección IA: inactiva — <motivo>` | ámbar (warn) |
| `Unknown`/arranque | `○ Corrección IA: comprobando…` | gris |

El motivo de inactividad es siempre visible (nunca silencioso): «el endpoint
https://… no respondió o rechazó la petición; el dictado sigue funcionando sin
corrección» (`remote_endpoint_reason`), «no se encontró el ejecutable del server
LLM (llama-server.exe). Configúralo en Ajustes o usa un endpoint
OpenAI-compatible…», etc.

### 9.2 Ajustes → Corrección IA (primer nivel)

- Línea de estado en vivo (misma lógica que el chip).
- Checkbox «Corrección IA: una IA revisa y corrige lo que dictas» + explicación:
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
  autodetección + hint de §5.

### 9.3 Ajustes avanzados (técnico, colapsado por defecto)

Cada opción lleva su explicación en lenguaje llano (texto idéntico al que se
muestra en la UI):

- **Calidad de transcripción (beam size)** — slider 1..=8, default 5: «Calidad
  de transcripción (beam size) vs velocidad: 1 = la más rápida, 8 = la más
  fiel. Rango 1–8, recomendado: 5.»
- **Núcleos de CPU** — slider 1..=16, default 4: «Núcleos de CPU que usa Whisper
  al transcribir. Sube si el equipo va justo de velocidad; baja para dejar
  recursos al resto. Rango 1–16, recomendado: 4.»
- **Espera máx. (s)** — timeout del LLM, slider 1..=120, default 25: «Espera
  máxima de la IA antes de dar el turno por definitivo: si lo supera, se inserta
  sin corregir. Rango 1–120 s, recomendado: 25 s.»
- **Sensibilidad** — umbral del clasificador 0..=100, default 50: subir = más
  conservador (menos llamadas al LLM, más dictado normal por defecto).

### 9.4 Corrección consciente

Mientras el LLM trabaja, `status = Status::Correcting` (ámbar, como
Transcribing): el usuario sabe que la app «ha oído» la corrección y está
trabajando. Aplicación por defecto: automática + aviso en el panel de feedback
(`last_correction`) + undo de 1 nivel. Toggle opcional **«Confirmar antes de
aplicar»** (`ai_correction_confirm`, default off): el diff llega con botones
«Aplicar / Descartar».

---

## 10. Latencia esperada

| Etapa | Coste estimado |
|---|---|
| Whisper Small (RTX 4070, CUDA) | tiempo real / ligeramente inferior (RTF ~0.2–0.5) |
| Clasificadores (regex) | < 1 ms |
| LLM 9B local, prompt ~250–400 tok, ~75 tok/s | ~4–6 s |
| Guard + aplicación + re-render | < 1 ms |

La corrección LLM añade **~4–6 s solo cuando se detecta intención correctiva**
(in-turn o cross-turn); el dictado normal no la paga. El timeout por defecto
(25 s) cubre ese margen con holgura para endpoints remotos.

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
  añadió t_c2b0a22f sobre el selector de proveedor y la degradación remota).
  Cubren: clasificadores
  cross-turn e in-turn (caso canónico de Juanfri «que calor en murcia digo
  cartagena» + variantes), `apply_diff` (numerales, «lo último», diff inválido
  no muta), undo, `clean_redact_output`, `contains_in_turn_marker` /
  `strip_in_turn_meta`, guard anti-truncación (casos largo y corto), guard
  numérico (§6.5: concatenación, numeral viejo en frase natural —los dos casos
  del log del usuario—, borrado de numerales, `no…sino`, ambigüedad que NO se
  toca y caso de control sin corrección), estado del
  chip, etiquetas de proveedor, defaults de `llm_settings` por proveedor.
- `cargo check` sin errores.
- Casos canónicos: cross-turn «la reunión es a las 4 en punto» + «no no lo
  último está mal, es 2 en vez de 4» → «la reunión es a las 2 en punto»;
  in-turn «que calor que hace en murcia digo cartagena» → «que calor que hace
  en cartagena».
- Endpoint remoto caído → `LlmDisabled` con aviso que nombra la URL y deja
  claro que el dictado sigue (test `endpoint_remoto_caido_desactiva_con_aviso_
  que_no_bloquea`).
- Toggle off → sin `llama-server.exe` en RAM/VRAM (solo el PID propio); Salir →
  idem.

## 13. Decisiones cerradas

- Motor: servidor **OpenAI-compatible vía ureq** (sin runtime local nuevo en el
  proceso nativo); el LLM corre en un hilo (`std::thread::spawn` + canal mpsc,
  nunca en el bucle de eventos ni en el hilo de audio).
- Alcance del diff cross-turn: `last_turn`; el transcript se actualiza solo si
  el target sigue presente.
- In-turn devuelve **texto final redactado**, no diff (el contenido del turno es
  el input y el output a la vez).
- Guard anti-truncación **en código, no en prompt** (9 variantes probadas en
  runtime; el prompt sin refuerzo preserva las correcciones).
- Guard numérico con **precisión > recall**: si el fragmento anterior al
  marcador tiene 2+ numerales, no se puede saber cuál rectifica el usuario y el
  guard no toca nada (el peor caso es inyectar el output del LLM o el texto
  bruto, nunca inventar una cifra). Cuando sí decide, su reconstrucción es el
  texto que se aplica e inyecta.
- Confirmación: off por defecto (aplicar + aviso + undo); `ai_correction_confirm`
  la activa desde Ajustes.
- API key exclusivamente en Credential Manager (Windows), `skip_serializing` en
  el JSON, migración legada al primer guardado.
- El server `llama-server.exe` se lanza/termina como proceso nativo con PID
  propio registrado; nunca se mata un server ajeno.