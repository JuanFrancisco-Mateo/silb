# CORRECCIÓN / FORMATEO POR VOZ — especificación

Objetivo: que el usuario pueda corregir lo que acaba de dictar **hablando**,
con frases como el ejemplo canónico:

> «no no lo último está mal, es 2 en vez de 4»

y que la app entienda la intención correctiva, localice el fragmento a cambiar
(el último texto dictado) y lo corrija en el transcript.

Esto es **post-procesado del texto transcrito**; no toca la captura de audio ni
la decodificación de Whisper. El dictado por turnos sigue igual.

---

## 1. Estado actual y por qué no alcanza

Ya existe un motor de rectificaciones **por reglas** en
`src/domain/correction.rs` (`apply_correction` + `CorrectionSession`
multi-turno). Cubre frases del tipo:

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

Conclusión: mantener las reglas como **fallback barato** y añadir una capa de
**post-procesado con LLM local** para los casos que las reglas no cubren.

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

## 3. Pipeline propuesto (post-procesado con LLM local)

### 3.1 Modelos candidatos

- **Ornith 9B** (~75 tok/s en RTX 4070) — opción principal, calidad alta.
- **Qwen** (p. ej. Qwen2.5 7B-Instruct) — alternativa más ligera, misma GPU.
- Ambas sirven de "correctores": prompt corto de ida y vuelta, sin generación
  larga. El nombre exacto del modelo se confirma al cablear la feature.

> Nota: en esta fase el LLM se ejecuta **solo cuando el clasificador detecta
> intención correctiva**, no en cada turno. El dictado normal no paga el coste.

### 3.2 Prompt correctivo (contrato)

Entrada al modelo (contexto + instrucción):

```
Eres el corrector de un dictado por voz en español. Se te da:
- TEXTO ACTUAL (el último turno dictado, ya limpio de muletillas).
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

## 5. Alternativa sin LLM (por reglas)

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

**Recomendación:** implementar primero el fallback por reglas (v1, sin
dependencias nuevas), validar la UX de confirmar/undo, y añadir el LLM (v2)
como camino para lo que las reglas no casen. Ambas capas comparten el mismo
contrato `AppliedDiff`, así que la v2 no reescribe la v1.

---

## 6. Integración en `app.rs` / `transcribe` sin romper el dictado realtime

Punto clave: el dictado es **por turnos** (grabar → transcribir → aplicar
pipeline), no streaming. La corrección se inserta **después** de
`apply_pipeline`, en `Message::TranscriptionDone`, y **nunca** en el hilo de
audio ni en el de whisper.

Cambios propuestos (todos aditivos):

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
4. **`src/transcribe.rs`**: **sin cambios**. La corrección consume el texto ya
   transcrito; no interfiere con `WhisperEngine`.

Lo que **no** debe pasar:

- No bloquear el bucle de eventos (el LLM corre en un hilo, el resultado llega
  por canal).
- No correr el LLM en el hilo de audio (competiría por la GPU con whisper y
  añadiría latencia al turno siguiente).
- No aplicar un diff inválido: si `find` no es subcadena literal, descartar.

---

## 7. Criterios de aceptación

- `cargo test` verde; nuevos tests en `corrective.rs`:
  - Clasificador: dictado normal no se marca como corrección (≥ 5 casos).
  - `apply_diff`: sustitución correcta de numerales y de «lo último» (≥ 5).
  - Diff inválido (`find` no presente) → no muta el texto.
  - Undo: restaura `before` tras `after`.
- El caso canónico «no no lo último está mal, es 2 en vez de 4» sobre
  «la reunión es a las 4 en punto» produce «la reunión es a las 2 en punto».
- `cargo check` sin errores; `src/transcribe.rs` y `src/audio.rs` intactos.
- Commit en español, por capa (v1 reglas, v2 LLM).

## 8. Decisiones abiertas

- Nombre exacto del modelo y motor de inferencia (llama-cpp vs candle vs
  servidor OpenAI-compatible). Se fija al empezar la v2.
- Alcance del diff: ¿solo `last_turn` o todo el transcript? v1 = `last_turn`.
- ¿Confirmación por defecto on u off? Propuesta: **off** (aplicar + aviso +
  undo), por fluidez.
