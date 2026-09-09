# CORRECCIÓN / FORMATEO POR VOZ — especificación

Objetivo: que el usuario pueda corregir lo que acaba de dictar **hablando**,
con frases como el ejemplo canónico:

> «no no lo último está mal, es 2 en vez de 4»

y que la app entienda la intención correctiva, localice el fragmento a cambiar
(el último texto dictado) y lo corrija en el transcript.

Esto es **post-procesado del texto transcrito**; no toca la captura de audio ni
la decodificación de Whisper. El dictado por turnos sigue igual.

---

## 1. Estado actual

La funcionalidad está implementada. Existe un motor de rectificaciones **por reglas** en
`src/domain/correction.rs` (`apply_correction` + `CorrectionSession`
multi-turno), complementado por un clasificador y un cliente OpenAI-compatible
(`src/domain/corrective.rs`, `src/llm.rs`). Cubre frases del tipo:

- «no, quería decir X» / «mejor dicho X» / «me refería a X»
- «donde dije X, quería decir Y» / «no era X, era Y»

Limitaciones frente al caso objetivo:

1. **Numerales.** El caso «es 2 en vez de 4» no casa con ningún patrón actual:
   las reglas esperan una reformulación completa («era el jueves»), no una
   sustitución parcial de un número dentro de la frase.
2. **Referencias anafóricas.** «lo último», «eso», «ese número» no se resuelven:
   no hay vínculo con el *contenido* del último turno.
3. **Negación doble / ruido.** «no no lo último está mal» no es una negación
   limpia de las que el regex reconoce.

Las reglas siguen siendo el **fallback barato**. Cuando el clasificador detecta
una intención correctiva que no resuelven, la app envía el turno a un proveedor
LLM configurable: local por defecto, cloud o endpoint personalizado.

---

## 2. Detección de intención correctiva

Antes de llamar al LLM (caro), un clasificador barato decide si el turno recién
transcrito es una **corrección** o **dictado normal**. Señales (en orden de
peso):

1. **Negaciones** al inicio: «no», «no no», «así no», «mal», «me equivoqué».
2. **Numerales** presentes en el turno: dígitos, palabras-número («dos»,
   «cuatro», «2», «4»), «en vez de», «en lugar de», «no ... sino ...».
3. **Referencias al texto previo**: «lo último», «eso», «esa palabra», «ese
   número», «la última frase», «lo que acabo de decir».
4. **Verbos de corrección**: «corrige», «cambia», «pon», «quita», «borra»,
   «sustituye».

Regla práctica de dos niveles:

- **Nivel A (seguro, sin LLM):** turnos que casan con `CORRECTION_PATTERNS`
  existente → `apply_correction` (comportamiento actual, sin cambios).
- **Nivel B (requiere LLM):** turnos con (negación **o** verbo correctivo) **y**
  (numeral **o** referencia anafórica) → enrutar al pipeline LLM descrito abajo.
- **Resto:** dictado normal → `append_transcript`.

Si el clasificador duda, la opción más segura es tratar el turno como dictado
normal (un falso positivo de corrección es peor que no corregir: borra texto
del usuario). El umbral debe favorecer la precisión sobre el recall.

---

## 3. Pipeline implementado (post-procesado con LLM)

### 3.1 Proveedores

- **Por defecto (local):** `http://127.0.0.1:8081/v1`, modelo `ornith-1.0-9b`.
- **Cloud:** OpenAI-compatible, por defecto `https://api.openai.com/v1`, modelo
  `gpt-4o-mini` y API key obligatoria.
- **Personalizado:** cualquier endpoint OpenAI-compatible y modelo indicado por
  el usuario; también admite Bearer API key.

La API key se mantiene fuera de `config.json`: en Windows se guarda en
Credential Manager. Los valores de configuración nuevos se escriben de forma
retardada para no bloquear la UI al editar campos o sliders.

> Nota: en esta fase el LLM se ejecuta **solo cuando el clasificador detecta
> intención correctiva**, no en cada turno. El dictado normal no paga el coste.

### 3.2 Prompt correctivo (contrato)

Entrada al modelo (contexto + instrucción):

```
Eres el corrector de un dictado por voz en español. Se te da:
- TEXTO ACTUAL (el último turno dictado, ya pasado por el post-procesado local).
- INSTRUCCIÓN (lo que el usuario dijo después, pidiendo una corrección).

Devuelve SOLO un JSON válido, sin texto adicional:
{"find": "<subcadena exacta en TEXTO ACTUAL>", "replace": "<nuevo texto>"}

Reglas:
- "find" debe ser una subcadena literal de TEXTO ACTUAL (puede ser una
  palabra, un número, o una frase corta). Si no puedes localizarla, devuelve
  {"find": "", "replace": ""}.
- "replace" es lo que sustituye a "find". Si la instrucción pide quitar algo,
  "replace" es "".
- No inventes texto que no esté en TEXTO ACTUAL ni en la INSTRUCCIÓN.
- Resuelve numerales: "2 en vez de 4" sobre "... 4 ..." => find "4", replace "2".
- Resuelve referencias: "lo último" apunta a la última frase/número de TEXTO ACTUAL.
```

Ejemplo de invocación:

```
TEXTO ACTUAL: "la reunión es a las 4 en punto"
INSTRUCCIÓN:   "no no lo último está mal es 2 en vez de 4"
→ {"find": "4", "replace": "2"}
```

### 3.3 Aplicar el diff

1. Validar que `find` es subcadena literal de `self.last_turn` (o del
   transcript, según el alcance). Si no lo es → **descartar la corrección** y
   tratar la instrucción como dictado normal (no romper el texto).
2. Sustituir **la primera ocurrencia** de `find` por `replace`. Para
   numerales/«lo último», normalmente es la última ocurrencia; el prompt puede
   pedir explícitamente que `find` sea único.
3. Registrar la operación en un **historial de undo**:
   `{ before, after, turn_index }`.
4. Actualizar `transcript` y `last_turn` con el texto corregido.

Contrato de código sugerido (nuevo módulo, sin tocar `audio` ni `whisper`):

```rust
// src/domain/corrective.rs
pub enum CorrectiveDecision {
    Plain,                        // dictado normal
    RuleCorrected(Correction),    // nivel A (correction.rs actual)
    NeedsLlm { turn: String },    // nivel B → pipeline LLM
}

pub struct AppliedDiff {
    pub find: String,
    pub replace: String,
    pub before: String,  // last_turn antes
    pub after: String,   // last_turn después
}

// clasificador barato + (opcional) llamada al LLM
pub fn classify(turn: &str) -> CorrectiveDecision { /* ... */ }
pub fn apply_diff(last_turn: &str, diff: &Diff) -> Option<AppliedDiff> { /* ... */ }
```

### 3.4 Latencia esperada

| Etapa | Coste estimado |
|---|---|
| Whisper Small (RTX 4070, CUDA) | tiempo real / ligeramente inferior (RTF ~0.2–0.5) |
| Clasificador (regex) | < 1 ms |
| LLM 9B, prompt ~250–400 tok, 75 tok/s | ~4–6 s |
| Aplicar diff + re-render | < 1 ms |

Latencia total añadida por la corrección LLM: **~4–6 s**, solo cuando se
detecta intención correctiva. Aceptable porque es un gesto explícito y poco
frecuente; el dictado normal no la paga. Si se quiere bajar, opciones: modelo
7B (~2–3 s), recorte de prompt, o stream del primer token.

---

## 3.5 Auto-corrección IN-TURNO (modo principal desde 10/09/2026)

### 3.5.1 Motivación

El modo cross-turn (§3.2) cubre correcciones en las que el usuario
dicta una frase separada apuntando a un turno anterior («no no lo último
está mal, es 2 en vez de 4»). Pero el caso más natural al dictar
fluidamente es **corregirse sobre la marcha**, en el mismo chorro de voz:

> «que calor que hace en murcia **digo cartagena**»

Sin soporte in-turn, la app escribe textualmente
«que calor que hace en murcia digo cartagena» — incluyendo la
meta-lengua correctiva, que NO debe aparecer en el texto final.

### 3.5.2 Detección

Nuevo clasificador `classify_in_turn(turn) -> bool` en
`src/domain/corrective.rs` que reconoce los marcadores típicos de
auto-corrección dentro del mismo turno:

- `digo X` / `diga X` / `dime X`
- `quiero decir X` / `quise decir X` / `mejor dicho X`
- `en vez de` / `en lugar de` (en contexto, no coloquial)
- `mejor:` / `mejor X que Y`

A diferencia de `classify` (§3.2), **no exige numeral ni referencia
anafórica**: el «antes» y el «después» viven en el mismo turno. La señal
correctiva basta por sí sola; el LLM resuelve qué sustituir.

Filtros de seguridad:
- Turno ≥ 6 chars y ≥ 3 chars antes del marcador y ≥ 1 char después
  (evita «digo sí», «digo», etc.).
- Kill-switch `classify_in_turn_with(turn, enabled)` para posibles
  toggles futuros en Ajustes.

### 3.5.3 Redacción del LLM (no diff)

Nuevo prompt `SYSTEM_PROMPT_IN_TURN` en `src/llm.rs`. El LLM recibe el
**TURNO COMPLETO** y devuelve el **TEXTO FINAL corregido** (sin
meta-lengua), NO un JSON `{find, replace}`. El modelo:

1. Detecta si hay auto-corrección en el turno.
2. Si la hay, aplica la corrección: lo anterior al marcador se
   sustituye por lo posterior.
3. **Elimina toda la meta-lengua** («digo X», «quiero decir», etc.).
4. Si NO hay auto-lengua, devuelve el texto tal cual.

Función: `redact(turn, cfg) -> Result<String, String>`. Mismo manejo
de errores que `correct()` (§3.4): timeout, errores HTTP, JSON
malformado, etc. → fallback al input bruto (spec §3.5.4).

Limpieza de la respuesta (`clean_redact_output`):
- Quita fences markdown (` ``` `).
- Quita prefijos tipo «Output:», «Resultado:».
- Quita comillas envolventes (rectas, tipográficas, «», etc.).
- Trim final.

### 3.5.4 Fallback seguro

Si el LLM falla, devuelve vacío, o devuelve texto que aún contiene
meta-lengua correctiva (`contains_in_turn_marker`), el flujo cae al
**input bruto**: el turno original se inyecta como un dictado normal
(spec §3.3.1 ampliado: nunca perder texto del usuario). El log
`%APPDATA%\whispr-open\whispr-open.log` registra el motivo exacto del
fallback.

### 3.5.5 Routing y prioridad

En `app.rs` (función `Message::TranscriptionDone`), el orden es:

1. **IN-TURN** (este modo, primario): si `classify_in_turn(processed)` →
   el LLM recibe el turno completo y devuelve el texto final.
2. **CROSS-TURN** (modo histórico, fallback): si
   `classify_with(processed, threshold) == NeedsLlm` y existe
   `last_turn` previo → el LLM recibe (last_turn, instruction) y
   devuelve `{find, replace}`.
3. **PLAIN**: el resto → dictado normal sin intervención del LLM.

Si un turno dispara AMBOS clasificadores (caso mixto, p. ej. «no no lo
último está mal es 2 en vez de 4» que tiene referencia previa Y
«en vez de»), gana el in-turn por prioridad.

### 3.5.6 Caso canónico

| Paso | Valor |
|---|---|
| Input (turno completo) | «que calor que hace en murcia digo cartagena» |
| `classify_in_turn` | true |
| LLM input | el turno completo |
| LLM output | «que calor que hace en cartagena» |
| Inyectado en app activa | «que calor que hace en cartagena» |

### 3.5.7 Latencia esperada

~4-6 s para un modelo 9B local (mismo orden que el cross-turn). El
dictado normal no la paga: solo se invoca el LLM si `classify_in_turn`
detecta auto-corrección.

---

## 4. UX (confirmar cambio / undo)

Principio del skill `app-design-2026`: **AI transparente y overrideable**. El
usuario debe ver qué cambió y poder deshacerlo.

1. **Preview del cambio** (no aplicar a ciegas): mostrar en el panel de
   feedback algo como `Rectificado: "4" → "2"` (reutilizar el patrón actual de
   `last_correction`).
2. **Confirmación opcional**: por defecto aplicar automáticamente + mostrar
   aviso. En ajustes, un toggle «Confirmar correcciones por voz» para que la
   app pregunte antes de aplicar (botón «Aplicar / Descartar»).
3. **Undo**: botón «Deshacer» que deshaga la última corrección aplicada
   (pila de `AppliedDiff`). Un solo nivel de undo es suficiente para la v1;
   la pila permite más sin coste.
4. **Estado nuevo en la UI**: `Status::Correcting` (ámbar, como
   `Transcribing`) mientras corre el LLM, para que el usuario sepa que la app
   "ha oído" la corrección y está trabajando.

---

## 5. Fallback sin LLM (por reglas)

Fallback y, a la vez, primera iteración barata. Cubre el caso canónico y
algunos vecinos con regex puros, sin LLM:

- Numeral + «en vez de» / «en lugar de»:
  `no[^,]*lo último[^,]*es (\d+) en vez de (\d+)` → sustituir `$2` por `$1` en
  `last_turn`.
- «era X no Y» / «X no Y» con numerales: `(?:era|es)\s+(\d+)\s+no\s+(\d+)`.
- Referencia explícita a «la última palabra» → sustituir la última palabra.

Ventajas: latencia ~0, determinista, testeable con unit tests (igual que
`correction.rs`). Desventajas: frágil ante variación («eso del final», «el
número ese», números en palabras «dos» vs «4»).

Las reglas y el LLM comparten el mismo contrato de diff. El clasificador barato
mantiene el coste del dictado normal prácticamente en cero y solo envía al LLM
las frases con señales suficientes de corrección.

---

## 6. Integración implementada en `app.rs` / `transcribe`

Punto clave: el dictado es **por turnos** (grabar → transcribir → aplicar
pipeline), no streaming. La corrección se inserta **después** de
`apply_pipeline`, en `Message::TranscriptionDone`, y **nunca** en el hilo de
audio ni en el de whisper.

Comportamiento implementado:

1. **`src/domain/corrective.rs`** (nuevo): `classify`, `apply_diff`, `Diff`,
   `CorrectiveDecision`. Lógica pura + tests (sin I/O, sin red).
2. **`src/llm.rs`** (nuevo, opcional v2): cliente del LLM local. Carga el
   modelo en un hilo de fondo, expone `fn correct(turn: &str, instruction:
   &str) -> Result<Diff, String>`. Usa `llama-cpp`/`candle` o un servidor
   OpenAI-compatible local. El modelo se carga una vez (lazy) y se reutiliza.
3. **`src/app.rs`**:
   - `apply_pipeline` delega en `classify` para decidir.
   - Nuevo `Message::CorrectionDone(Result<AppliedDiff, String>)` y
     `Message::UndoLastCorrection`.
   - En `TranscriptionDone`, si `classify` devuelve `NeedsLlm`, se manda el
     trabajo a un hilo (`std::thread::spawn` + canal, igual que la
     transcripción) y se pone `status = Status::Correcting`; al volver el
     resultado, se aplica el diff y se actualiza el estado.
   - El `Poll` de 100 ms ya drena `transcribe_rx`; añadir un `correct_rx`
     análogo (mismo patrón, sin bloquear).
4. **`src/transcribe.rs`** mantiene separadas la transcripción y la corrección;
   además, el contexto de Whisper se cachea por modelo para evitar recargas por
   turno.

Lo que **no** debe pasar:

- No bloquear el bucle de eventos (el LLM corre en un hilo, el resultado llega
  por canal).
- No correr el LLM en el hilo de audio (competiría por la GPU con whisper y
  añadiría latencia al turno siguiente).
- No aplicar un diff inválido: si `find` no es subcadena literal, descartar.

---

## 7. Criterios de aceptación

- `cargo test` verde; tests en `corrective.rs` y `llm.rs`:
  - Clasificador cross-turn: dictado normal no se marca como
    corrección (≥ 5 casos). Modo in-turn: caso canónico de Juanfri
    «que calor en murcia digo cartagena» + variantes.
  - `apply_diff` (cross-turn): sustitución correcta de numerales y de
    «lo último» (≥ 5).
  - Diff inválido (`find` no presente) → no muta el texto.
  - Undo: restaura `before` tras `after`.
  - `clean_redact_output`: fences, comillas (rectas + tipográficas),
    prefijos, combinaciones.
  - `contains_in_turn_marker` / `strip_in_turn_meta`: red de seguridad.
- Caso canónico cross-turn: «no no lo último está mal, es 2 en vez de 4»
  sobre «la reunión es a las 4 en punto» produce «la reunión es a las
  2 en punto».
- Caso canónico in-turn (t_78d9e1fa): «que calor que hace en murcia
  digo cartagena» produce «que calor que hace en cartagena» (la
  meta-lengua desaparece; el LLM redacta, no aplica diff).
- `cargo check` sin errores; `src/transcribe.rs` y `src/audio.rs` intactos.
- Commit en español, por capa (v1 reglas, v2 LLM, v3 in-turn).

## 8. Decisiones cerradas

- Motor: servidor OpenAI-compatible mediante `ureq`, sin añadir otro runtime
  local al proceso nativo.
- Alcance del diff: `last_turn`; el transcript se actualiza solo si el target
  capturado sigue presente.
- Confirmación: off por defecto (aplicar + aviso + undo); puede activarse desde
  Ajustes para mostrar «Aplicar / Descartar».
