# Whispr Open

Base open source para una aplicación de dictado natural inspirada en el flujo de Whispr Flow. La captura y transcripción se ejecutan localmente en el navegador mediante whisper.cpp compilado a WebAssembly; el borrador y el modelo se mantienen en el dispositivo.

## Ejecutar

Requisitos: Node.js 20 o superior.

```bash
npm install
npm run dev
```

Abre la URL que muestre Vite. El dictado requiere permiso de micrófono, IndexedDB y un navegador con WebAssembly SIMD y `SharedArrayBuffer`. Vite envía las cabeceras COOP/COEP necesarias para activar el aislamiento de origen.

Para generar una build de producción:

```bash
npm run build
npm run preview
```

## Pruebas

La suite del motor corre sin navegador, micrófono ni modelo Whisper:

```bash
npm test
npm run test:watch
```

Cubre correcciones completas, puntuación, discurso normal, múltiples marcadores naturales en español, rectificaciones divididas en dos y tres turnos, falsos inicios y reset del estado pendiente. También verifica el ciclo de grabación simulado, la liberación del micrófono, el cache del modelo Whisper, el remuestreo, el progreso y los errores de compatibilidad.

## Aplicación de escritorio

La base incluye un shell Tauri v2 para Windows, macOS y Linux. Requiere Rust, WebView2 en Windows y las dependencias de desarrollo nativas de Tauri en cada plataforma.

```bash
npm run tauri:dev
```

Para compilar el ejecutable sin generar todavía instaladores:

```bash
npm run tauri build -- --no-bundle
```

Para generar los instaladores de la plataforma:

```bash
npm run tauri:build -- --no-sign
```

El atajo global `Ctrl/Cmd + Shift + D` muestra la ventana y alterna el dictado aunque la aplicación no tenga el foco. El evento nativo llega a React mediante `global-dictation-toggle` y reutiliza el mismo flujo de grabación local con Whisper WASM.

## Escritura en otras apps (Windows)

En el escritorio, el texto transcrito puede escribirse directamente en la aplicación activa (Slack, Notion, editor, terminal) en lugar de solo en la ventana de Whispr Open. El comando nativo `type_text` copia el texto al portapapeles con `arboard` y simula la pulsación `Ctrl+V` con `enigo`, de modo que el pegado ocurre en la app que tenga el foco, fuera de la ventana Tauri.

- Solo disponible en Windows; en otras plataformas el comando devuelve un error controlado.
- En la versión web (sin el runtime Tauri) la llamada falla de forma silenciosa con `{ ok: false, error: 'web' }`, sin interrumpir el flujo de dictado.
- El texto sustituye temporalmente el contenido del portapapeles (el pegado lo consume la app activa).

## Qué incluye

- Editor de texto con transcripción por turnos mediante `MediaRecorder`.
- Whisper.cpp WASM local con modelo configurable (`tiny`/`base`/`small`), descargado una vez y cacheado en IndexedDB. El modelo `small` descarga ~460 MB la primera vez (frente a ~75 MB de `tiny`); `base` se queda en un punto intermedio.
- Progreso visible de descarga, remuestreo e inferencia; el audio no se envía a un backend.
- Detección local de rectificaciones como `Uy, así no era, era el informe del jueves`, `perdón, quería decir...`, `mejor dicho...`, `donde dije X, quería decir Y`, `me refería a...` y `no, el jueves`.
- Diccionario personal de términos → reemplazo (jerga, nombres propios, siglas) aplicado sobre cada transcripción y persistido en `localStorage`.
- Panel de actividad con el texto original y la versión aplicada.
- Guardado explícito del borrador en `localStorage`.
- Interfaz responsive sin cuenta, backend ni telemetría.
- Aplicación de escritorio Tauri v2 con atajo global para iniciar o detener dictado.
- Escritura del dictado en la aplicación activa en Windows (`type_text`: portapapeles + `Ctrl+V` simulado).

## Arquitectura

- `src/App.tsx`: flujo de interfaz, grabación y estados del motor.
- `src/services/local-whisper.ts`: disponibilidad, descarga/cache del modelo, remuestreo e inferencia WASM.
- `src/domain/correction.ts`: contrato, reglas puras y `CorrectionSession` para rectificaciones divididas por turnos.
- `src/domain/dictionary.ts`: diccionario personal término → reemplazo (jerga, nombres propios), con persistencia en `localStorage`.
- `src/services/model-config.ts`: selector de modelo Whisper (`tiny`/`base`/`small`) con persistencia en `localStorage`.
- `src/services/text-injection.ts`: inyección del texto transcrito en la aplicación activa vía el comando Tauri `type_text`, con fallback web silencioso.
- `src-tauri/src/lib.rs`: shell Tauri v2 (plugin global-shortcut, comando `type_text`).
- `src/domain/correction.test.ts`: suite unitaria del motor de corrección.
- `src/services/recording-controller.ts`: ciclo de grabación y transcripción con dependencias inyectables.
- `src/services/recording-controller.test.ts`: integración simulada de micrófono, `MediaRecorder` y transcriptor.
- `src/services/local-whisper.test.ts`: contrato del servicio WASM con primitivas Whisper simuladas.
- `src/styles.css`: sistema visual y layout responsive.

## Próximas iteraciones

1. Pasar de transcripción por turnos a ventanas de audio solapadas para mostrar resultados casi en tiempo real.
2. Añadir un `CorrectionEngine` intercambiable con reglas, historial de contexto y un modelo local de lenguaje.
3. Capturar comandos de rectificación entre turnos: detectar “no, quería decir...” aunque la frase llegue dividida en varios resultados.
4. Incorporar pruebas de integración del flujo de voz y del atajo global de escritorio.
5. Añadir selección de idioma, puntuación y formato configurable por perfil.

## Modelo y licencias

`@remotion/whisper-web` se usa como wrapper WASM de whisper.cpp y se instala fijado en `4.0.516`. El modelo `tiny` se descarga desde el proveedor configurado por el paquete durante el primer uso y se almacena en IndexedDB; si se necesita una distribución completamente reproducible/offline, el siguiente paso es servir el modelo desde `public/models` y revisar las licencias de los pesos antes de redistribuirlos.

La aplicación sigue pendiente de elegir una licencia de repositorio. MIT es una opción razonable para esta base, separada de las licencias de whisper.cpp y de los pesos del modelo.
