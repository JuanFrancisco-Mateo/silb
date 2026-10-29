# SETUP — Corrección IA: proveedores Local, Custom y Cloud

Guía corta para dejar funcionando el corrector con IA de Silb. El diseño
completo (por dónde decide, cómo libera memoria, la protección contra respuestas
recortadas) está en `docs/CORRECCION-VOZ.md`; aquí solo «cómo se configura cada
proveedor».

La app **se apaña sola** por defecto: al abrirla descarga el modelo de voz de
Whisper si falta y arranca el servidor de IA local si lo encuentra. Si en tu PC
el servidor local no aparece automáticamente, sigue la sección Local; si quieres
usar tu propio servidor compatible con OpenAI o la nube de OpenAI, las secciones
Custom y Cloud.

---

## Local (llama.cpp — el que viene con la app) [DEFAULT]

Sin internet y sin API key: la app arranca `llama-server.exe` al abrirse y le
pasa el modelo GGUF. Proveedor en Configuración → Corrección IA → **Local**.

### 1. Conseguir el modelo GGUF

Desde la 0.7 **no hay que bajar nada a mano**: en Ajustes → Corrección IA →
Local, el desplegable **Modelo IA local** trae tres modelos pequeños
(Qwen2.5-0.5B ~491 MB, el recomendado; Llama 3.2 1B ~808 MB; Qwen2.5-1.5B
~1,1 GB) y el botón **Descargar** los baja solo a
`%APPDATA%\whispr-open\models-llm\`. Con «Otro (repo o URL de Hugging Face)…» se
pega el `owner/repo` o la dirección de un `.gguf` público; solo se aceptan GGUF y
si el repo no trae ninguno, se avisa. Las descargas a medias no se reanudan: cada
intento empieza de cero.

Si ya tienes un GGUF en el equipo, la app lo encuentra por **nombre de archivo
exacto** (baja hasta 6 niveles) en estas rutas, en este orden:

- `%APPDATA%\whispr-open\models-llm\` (la carpeta propia del corrector)
- `%USERPROFILE%\.lmstudio\models\...` (la carpeta de modelos de LM Studio,
  incluida `lmstudio-community\...`)
- `D:\LlamaCPP\models\...` o `C:\LlamaCPP\models\...`
- `D:\models\...` o `C:\models\...`

En «Otro» puedes **escribir ese nombre de archivo** (se usa el que ya tienes, sin
descargar nada) o pegar el `owner/repo` / la dirección `.gguf` de Hugging Face
para que la app se lo baje a `models-llm/`.

### 2. Localizar llama-server.exe

La búsqueda automática mira, en orden:
`D:\LlamaCPP\llama-server.exe` / `C:\LlamaCPP\llama-server.exe` → junto al exe
de la app → `%LOCALAPPDATA%\LlamaCPP\llama-server.exe` → PATH de Windows.

- Si tienes una compilación de llama.cpp (servidor CUDA o CPU), copia
  `llama-server.exe` a `D:\LlamaCPP\` (con sus DLL al lado:
  `llama-server-impl.dll`, `cublas64_13.dll` o las `ggml-*` que
  correspondan — el exe las carga desde su propia carpeta) y listo.
- Si la app **no lo encuentra sola**, escríbelo en Ajustes → Corrección IA →
  **Server LLM** → «Examinar…» (lo más fácil) o pega la ruta completa. El campo
  muestra «auto: D:\LlamaCPP\llama-server.exe» cuando lo ha resuelto solo.

### 3. Comprobar

- Abre la app: el aviso de estado debe pasar de «comprobando…» a
  **«● Corrección IA: activa · ornith-1.0-9b»**.
- El registro del servidor (si algo falla) está en
  `%APPDATA%\whispr-open\llama-server.log`.
- Si apagas la corrección con IA, **el modelo se descarga de la memoria**
  (el servidor se cierra); si la vuelves a encender, se carga otra vez. Al salir
  de la app el servidor también se cierra.

---

## Custom (cualquier servidor OpenAI-compatible)

Proveedor en Configuración → Corrección IA → **Custom LLM**. Sirve para
llama.cpp en otra máquina, vLLM, LM Studio en modo servidor, Groq, Gemini (con
su propio base_url), etc.

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
el endpoint no responde o rechaza la petición, la corrección se desactiva con un
aviso a la vista y el dictado sigue con el texto tal cual (nunca te bloquea).

---

## Cloud (OpenAI)

Proveedor en Configuración → Corrección IA → **Cloud (OpenAI)**.

| Campo | Qué poner |
|---|---|
| Endpoint LLM | `https://api.openai.com/v1` (default; puedes cambiarlo) |
| Modelo IA | `gpt-4o-mini` (default) u otro |
| API key | **obligatoria** — tu key de OpenAI. Se guarda en el Administrador de credenciales de Windows, nunca en config.json |

Nota: con Cloud tu dictado se envía a los servidores de OpenAI para corregirlo.
Si quieres que no salga nada de tu máquina, usa Local.

---

## Resolución de problemas rápida

| Síntoma | Causa probable |
|---|---|
| «no se encontró el ejecutable del server LLM (llama-server.exe)» | falta `llama-server.exe` en las rutas de autodetección → descárgalo o pon la ruta en Server LLM |
| «no se encontró el modelo LLM «ornith-1.0-9b» en las rutas habituales» | el GGUF no está en las carpetas buscadas → muévelo o escribe el nombre/ruta en Modelo IA |
| «el endpoint https://… no respondió o rechazó la petición» | URL o API key mal → prueba con curl (arriba) |
| «el servidor LLM (…) no respondió a /v1/models en 60 s» | el server lanza pero no carga el modelo → mira `%APPDATA%\whispr-open\llama-server.log` |
| Correctas las opciones pero el chip dice «inactiva» | activa el toggle de Corrección IA y vuelve a comprobar; si el endpoint cayó a mitad, corrígelo y dicta de nuevo |

Todas las rutas y nombres de función citados están comprobados en el código
(rama `native-iced`); no hay ningún comportamiento distinto del descrito.
