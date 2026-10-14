# SETUP — Corrección IA: proveedores Local, Custom y Cloud

Guía breve para dejar funcionando el corrector IA de Silb. El detalle
del diseño (routing, ciclo de vida de memoria, guard anti-truncación) está en
`docs/CORRECCION-VOZ.md`; aquí solo el «cómo se configura cada proveedor».

La app **arranca sola** por defecto: al abrirla, descarga el modelo de voz de
Whisper si falta y lanza el server LLM local si lo encuentra. Si en tu PC el
server local no se encuentra automáticamente, sigue la sección Local; si quieres
usar tu propio servidor OpenAI-compatible o la nube de OpenAI, secciones Custom
y Cloud.

---

## Local (llama.cpp — el que viene con la app) [DEFAULT]

Sin internet y sin API key: la app lanza `llama-server.exe` al arrancar y le
pasa el modelo GGUF. Proveedor en Configuración → Corrección IA → **Local**.

### 1. Descargar el modelo GGUF

El modelo por defecto es `ornith-1.0-9b`. Si tu equipo va justo de VRAM,
cualquier GGUF pequeño de un modelo instruct de calidad funciona igual (cambia
el campo Modelo IA).

- Bájalo de Hugging Face (p. ej. un repo de `lmstudio-community` para que la
  autodetección lo encuentre) o del sitio que prefieras.
- Colócalo en cualquiera de estas rutas (la app las busca en este orden):
  - `%USERPROFILE%\.lmstudio\models\...` (la carpeta de modelos de LM Studio,
    incl. `lmstudio-community\...`)
  - `D:\LlamaCPP\models\...` o `C:\LlamaCPP\models\...`
  - `D:\models\...` o `C:\models\...`
- La búsqueda es por **nombre de archivo exacto** y recorre hasta 6 niveles de
  profundidad: si tu GGUF se llama distinto, cambia el campo **Modelo IA** en
  Ajustes (p. ej. `qwen3-0.8b-q8_0.gguf`) o pon la ruta completa al archivo.

### 2. Localizar llama-server.exe

La autodetección busca, en orden:
`D:\LlamaCPP\llama-server.exe` / `C:\LlamaCPP\llama-server.exe` → junto al exe
de la app → `%LOCALAPPDATA%\LlamaCPP\llama-server.exe` → PATH de Windows.

- Si tienes una build de llama.cpp (server CUDA/CPU), copia `llama-server.exe`
  a `D:\LlamaCPP\` (con sus DLL del lado: `llama-server-impl.dll`,
  `cublas64_13.dll` o las `ggml-*` correspondientes — el exe las carga desde su
  propio directorio) y listo.
- Si la app **no lo encuentra sola**, escríbelo en Ajustes → Corrección IA →
  **Server LLM** → «Examinar…» (más fácil) o pega la ruta completa. El campo
  muestra «auto: D:\LlamaCPP\llama-server.exe» cuando está resuelto solo.

### 3. Comprobar

- Abre la app: el chip debe pasar de «comprobando…» a
  **«● Corrección IA: activa · ornith-1.0-9b»**.
- El log del server (si algo falla) está en
  `%APPDATA%\whispr-open\llama-server.log`.
- El toggle de Corrección IA apagado **descarga el modelo de memoria** (termina
  el server); encendido lo vuelve a cargar. Al salir de la app el server también
  se cierra.

---

## Custom (cualquier servidor OpenAI-compatible)

Proveedor en Configuración → Corrección IA → **Custom LLM**. Sirve para
llama.cpp en otra máquina, vLLM, LM Studio en modo server, Groq, Gemini
(base_url propia), etc.

### Campo a campo

| Campo | Qué poner |
|---|---|
| Endpoint LLM | `http://127.0.0.1:8000/v1`, `https://api.groq.com/openai/v1`, `https://generativelanguage.googleapis.com/v1beta/openai`… |
| Modelo IA | el id del modelo en ese servidor (p. ej. `llama-3.3-70b-versatile`) |
| API key | solo si el endpoint la exige (opcional). Se guarda en el Administrador de credenciales de Windows, **nunca** en config.json |

### Probar el endpoint antes de configurarlo (curl)

```bash
# 1) el server responde y lista modelos (con key si la exige):
curl -s http://127.0.0.1:8000/v1/models
curl -s -H "Authorization: Bearer TU_KEY" https://tu-servidor/v1/models

# 2) el endpoint corrige (mismo contrato que usa la app):
curl -s http://127.0.0.1:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "tu-modelo",
    "messages": [
      {"role": "system", "content": "Eres un corrector de dictado por voz en español."},
      {"role": "user", "content": "que calor que hace en murcia digo cartagena"}
    ],
    "max_tokens": 256
  }'
```

Si `GET /v1/models` devuelve 200, la app lo verá como «activa» al arrancar. Si
el endpoint no responde o rechaza, la corrección se desactiva con aviso visible
y el dictado sigue a texto bruto (nunca bloquea).

---

## Cloud (OpenAI)

Proveedor en Configuración → Corrección IA → **Cloud (OpenAI)**.

| Campo | Qué poner |
|---|---|
| Endpoint LLM | `https://api.openai.com/v1` (default; puedes cambiarlo) |
| Modelo IA | `gpt-4o-mini` (default) u otro |
| API key | **obligatoria** — tu key de OpenAI. Se guarda en el Administrador de credenciales de Windows, nunca en config.json |

Nota: con Cloud tu dictado se envía a los servidores de OpenAI para la
corrección. Si quieres que nada salga de tu máquina, usa Local.

---

## Resolución de problemas rápida

| Síntoma | Causa probable |
|---|---|
| «no se encontró el ejecutable del server LLM (llama-server.exe)» | falta `llama-server.exe` en las rutas de autodetección → descárgalo o pon la ruta en Server LLM |
| «no se encontró el modelo LLM «ornith-1.0-9b» en las rutas habituales» | el GGUF no está en las carpetas buscadas → muévelo o escribe el nombre/ruta en Modelo IA |
| «el endpoint https://… no respondió o rechazó la petición» | URL o API key mal → prueba con curl (arriba) |
| «el servidor LLM (…) no respondió a /v1/models en 60 s» | el server lanza pero no carga el modelo → mira `%APPDATA%\whispr-open\llama-server.log` |
| Correctas las opciones pero el chip dice «inactiva» | activa el toggle de Corrección IA y vuelve a comprobar; si el endpoint cayó a mitad, corrígelo y dicta de nuevo |

Todas las rutas y nombres de función citados están verificados en el código
(rama `native-iced`); no hay ningún comportamiento distinto del descrito.