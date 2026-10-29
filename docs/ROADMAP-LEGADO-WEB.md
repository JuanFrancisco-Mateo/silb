# (HISTÓRICO) Roadmap de la versión web legada — rama `main`

> **Este documento es histórico.** Describe el plan de la versión web
> (React/TS + Tauri v2 + Whisper WASM, rama `main`): habla de `App.tsx`, `npm`
> y agentes A/B/C. Nada de eso existe en la rama activa.
>
> El roadmap **al día** de la app nativa (rama `native-iced`) es
> [`ROADMAP.md`](ROADMAP.md), en esta misma carpeta `docs/`; el `ROADMAP.md`
> de la raíz solo apunta ahí.
> Se guarda solo como referencia de cómo se llegó hasta aquí; sus números
> (40 tests, `npm test`) son de la versión web y no describen la app actual.
>
> Texto original, sin retocar, a continuación.

---

# ROADMAP — Whispr Open → paridad razonable con Wispr Flow

Objetivo: cerrar las 3 brechas principales frente a la app de pago Wispr Flow,
en una sesión nocturna con agentes autónomos. Priorizado por impacto/riesgo.

Estado verificado al inicio (01/09/2026): tests 40/40 ✓, build web ✓,
Tauri compilable (deps presentes), SIN git (al iniciar la sesión).

Leer `AGENTS.md` primero. Todas las verificaciones con `cmd.exe /c`.

---

## Regla de reparto de ficheros (evitar conflictos en paralelo)

| Fichero | Quién lo toca |
|---|---|
| `src/App.tsx`, `src/styles.css` | SOLO AGENTE INTEGRADOR |
| `src/domain/cleanup.ts` + test | SOLO AGENTE A |
| `src/domain/dictionary.ts` + test | SOLO AGENTE B |
| `src/services/model-config.ts` + test, `src/services/local-whisper.ts` | SOLO AGENTE B |
| `src-tauri/*`, `src/services/text-injection.ts` | SOLO AGENTE C |
| `package.json`, `Cargo.toml` | Solo si es imprescindible + avisar en commit |

Los agentes A/B/C entregan MÓDULOS + TESTS, nunca cablean UI en App.tsx.
El integrador cablea toda la UI al final (fase 3) y verifica el conjunto.

---

## FASE 0 — Base git (AGENTE INTEGRADOR, ~10 min)

- Si no hay repo: `git init -b main` en la raíz + `.gitignore` con
  `node_modules/`, `dist/`, `src-tauri/target/`.
- Commit inicial "chore: baseline whispr-open (tests 40/40)".
- Verificar baseline: `cmd.exe /c "cd /d whispr-open && npm test"`
  → 40 tests pasando. Registrar el número en el informe final.

ACEPTACIÓN: repo con commit inicial, tests 40/40 desde Windows.

---

## FASE 1 — Motor de limpieza de muletillas (AGENTE A, paralelo a Fase 2 y 3)

Nuevo módulo `src/domain/cleanup.ts` (funciones puras) + `src/domain/cleanup.test.ts`.

Objetivo funcional (modelado en Wispr Flow: "Filler identified / Repetition identified"):
- Detectar y eliminar muletillas comunes del habla transcrita:
  es: "eh", "um", "este", "esto", "o sea", "tipo", "bueno" (inicio), "pues"
  en: "uh", "um", "like", "you know"
- Detectar y colapsar repeticiones de palabra o bigrama ("el el", "the the",
  "que que", "la la") a una sola ocurrencia.
- Mantener puntuación y capitalización del transcript original (no reescribir frases).
- API pública: `cleanTranscript(text: string): { text: string; removed: string[] }`
  donde `removed` lista lo eliminado (para mostrarlo en UI tipo "Filler identified").
- Reglas en un array de patrones, MISMO estilo que `src/domain/correction.ts`.
- NO tocar `App.tsx`. NO tocar el motor de corrección existente.

ACEPTACIÓN:
- `cleanup.test.ts` con ≥ 15 casos (muletillas es/en, repeticiones, frases limpias,
  edge cases: mayúsculas, puntuación, strings vacíos).
- `cmd.exe /c npm test` → 40 baseline + nuevos, todos verdes.
- No cambia el comportamiento de `correction.ts`.

---

## FASE 2 — Diccionario personal + modelo configurable (AGENTE B, paralelo)

### 2a. `src/domain/dictionary.ts` + test
- Mapa término → reemplazo (jerga, nombres propios, siglas).
  Ej: "whatsapp" → "WhatsApp", "open ai" → "OpenAI", "lidl" → "LIDR".
- Función pura `applyDictionary(text: string, entries: DictionaryEntry[]): { text: string; applied: string[] }`.
- `DictionaryEntry = { from: string; to: string }`; matching case-insensitive
  con palabra completa (bordes de palabra), preservando capitalización de `to`.
- Persistencia: localStorage `whispr-open-dictionary` (JSON array) — helpers
  `loadDictionary()/saveDictionary()` en el MISMO módulo, con try/catch.
- Tests ≥ 10 casos. NO tocar `App.tsx`.

### 2b. `src/services/model-config.ts` + test
- Selector de modelo Whisper: `tiny` (actual), `base`, `small`.
- Constante de opciones con label en español: `[{ id: 'tiny', label: 'Tiny (rápido)' }, { id: 'base', ... }, { id: 'small', label: 'Small (preciso)' }]`.
- Persistencia: localStorage `whispr-open-model`.
- `getModel()/setModel(id)` con validación (id desconocido → tiny).
- MODIFICACIÓN MÍNIMA en `src/services/local-whisper.ts`: `LOCAL_WHISPER_MODEL`
  pasa de constante a leído vía `getModel()`. Los tests existentes de
  local-whisper son mock — deben seguir pasando (ajustar el mock si hace falta).
- Aviso en commit: modelo `small` descarga ~460 MB la primera vez (documentar en README).
- Tests ≥ 5 casos.

ACEPTACIÓN: tests verdes, `npm run build` OK, `local-whisper.test.ts` intacto en espíritu.

---

## FASE 3 — Inyección de texto en la app activa (AGENTE C, paralelo) — LA FEATURE ESTRELLA

Objetivo (gap nº1 con Wispr Flow: "types wherever your cursor is"):
al terminar un dictado con el atajo global, el texto se escribe en la app
que tenga el foco (Slack, Notion, editor, terminal), no solo en la ventana propia.

Implementación (Windows/Tauri; la versión web queda sin esta feature):
- **NOTA: `arboard` y `enigo` YA están añadidos a `Cargo.toml`** (pre-warmup).
  No volver a añadirlos; úsalos. Verificar con `cargo tree` si hace falta.
- Comando Tauri en `src-tauri/src/lib.rs` (o módulo nuevo): `type_text(text: String)`.
- Estrategia recomendada (lazy y robusta): poner `text` en el portapapeles
  (crate `arboard`) y simular la pulsación Ctrl+V con `enigo`.
  Alternativa si enigo falla: devolver el texto vía command separado y que la UI
  copie al portapapeles (navigator.clipboard) — pero el pegado tiene que ocurrir
  FUERA de la ventana Tauri, así que la simulación de teclado es imprescindible.
- Solo Windows por ahora (`cfg!(target_os = "windows")`); en otras plataformas el
  command devuelve error controlado.
- Nuevo wrapper TS: `src/services/text-injection.ts` con
  `injectTextIntoActiveApp(text: string): Promise<{ ok: boolean; error?: string }>`
  usando `invoke('type_text', { text })`. Si no hay `window.__TAURI_INTERNALS__`
  → devolver `{ ok: false, error: 'web' }` (silencioso).
- Registrar el permission del command en `src-tauri/capabilities/default.json`
  si Tauri v2 lo exige (verificar con cargo check).
- NO tocar `App.tsx`.

ACEPTACIÓN:
- `cmd.exe /c "cargo check"` (en `src-tauri/`; compila Windows).
- `src/services/text-injection.ts` con test que mockea `window.__TAURI_INTERNALS__`
  (invoke capturado) y verifica argumentos + fallback web.
- README: nueva sección "Escritura en otras apps (Windows)".

> ⚠️ OVERRIDE cargo (CRÍTICO): `cmd.exe` NO tiene cargo en PATH. En TODAS las
> verificaciones de Rust usar la ruta completa:
> `cmd.exe /c "cargo check"` (o `build`).
> La versión "cargo check" suelta falla en cmd.exe.

---

## FASE 4 — Integración UI y verificación final (AGENTE INTEGRADOR, al final)

SOLO el integrador toca `src/App.tsx` y `src/styles.css`. Cablear, en orden:

1. Fase 1 → toggle "Limpiar muletillas" (checkbox) junto al botón de dictado;
   al activarse, aplicar `cleanTranscript` al resultado de cada turno ANTES de
   `appendTranscript`/`replaceLastSentence`, y mostrar `removed` en el panel
   de actividad (tipo "Filler identified").
2. Fase 2a → botón/panel "Diccionario": lista de entradas, añadir/borrar,
   persistencia localStorage. Aplicar siempre el diccionario tras `cleanTranscript`.
3. Fase 2b → selector de modelo en el footer/config (label + tamaño de descarga).
4. Fase 3 → en escritorio, cuando `injectTextIntoActiveApp` devuelve ok=false por
   estar en web, no mostrar nada; si error real, notice en el status.
5. `src/styles.css`: estilos coherentes con el sistema existente (variables
   CSS / clases actuales, no rediseñar).

Verificación final obligatoria (en orden):
- `cmd.exe /c npm test` → TODOS verdes (baseline + nuevos).
- `cmd.exe /c npm run build` → sin errores.
- `cmd.exe /c "cd /d whispr-open\src-tauri && cargo check"` → OK (si Fase 3 tocó Rust).
- Commit por cada fase integrada (mensajes en español, p.ej.
  `feat: limpieza de muletillas conectada a la UI`).

ACEPTACIÓN GLOBAL: los 3 puntos anteriores verdes + informe final.

---

## Informe final (AGENTE INTEGRADOR)

Escribir `INFORME-NOCTURNO.md` en la raíz con:
- Resultado de cada fase (hecho/no hecho, tests añadidos, commits).
- Qué quedó pendiente y por qué.
- Verificación final con los números reales (tests totales, tamaños de chunk).
- Instrucciones para que el usuario pruebe en mano (qué features hay y cómo se activan).

## Fuera de alcance esta noche (no intentarlo)

- Transcripción casi-en-tiempo-real con ventanas solapadas (riesgo alto, toca el
  núcleo de grabación; requiere prueba manual de micrófono).
- Notetaker de reuniones / identificación de hablantes.
- Builds de instaladores (MSI/NSIS) — pueden romperse por firma; solo `cargo check`.
- Android/móvil.
