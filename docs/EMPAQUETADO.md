# EMPAQUETADO Y DISTRIBUCIÓN — especificación

Objetivo: convertir `target/release/whispr-open.exe` en distribuciones
instalables y confiables para Windows 10/11. Estado actual: solo existe el
`.exe` de release (~43 MB, sin consola), sin instalador, sin icono de archivo,
sin firma de código.

---

## 1. Formas de distribución (de más simple a más completa)

### 1.1 Portable `.exe` (recomendado empezar aquí)

Un único ejecutable autocontenido, sin instalación.

- Qué hace falta: incrustar el **icono** en el binario (recursos de Windows,
  ver §4), y empaquetar cualquier DLL de runtime que falte.
- Cómo: `cargo build --release` ya produce un binario estático salvo las
  DLLs de MSVC runtime y las de CUDA/whisper. Verificar con `dumpbin`/`llvm-nm`
  qué DLLs importa y decidir si se enlazan estáticamente o se redistribuyen
  (carpeta junto al `.exe`).
- Ventajas: sin elevación de privilegios, ideal para probar y para "probar en
  mano". Desventajas: sin asociación de archivos, sin integración con el menú
  Inicio, sin auto-actualización.

### 1.2 Instalador (NSIS / Inno Setup / WiX)

| Herramienta | Ventajas | Contras |
|---|---|---|
| **Inno Setup** | Script `.iss` declarativo y legible; firma integrada (`SignTool`); compresión LZMA buena; gratis. | Solo Windows (está bien aquí). |
| **NSIS** | Muy ligero, plugins maduros (auto-update, MUI). | Scripting menos legible que Inno. |
| **WiX** | Integración MSI/nativa con Windows; la vía "oficial" para MSI. | XML verboso, curva de aprendizaje mayor. |

**Recomendación:** Inno Setup para la v1. Cubre: instalar a
`%LOCALAPPDATA%\Programs\WhisprOpen\`, accesos directos (menú Inicio +
escritorio), desinstalador, e integración con `SignTool` para firmar en el
mismo pipeline.

Contenido del instalador:

- `whispr-open.exe` (+ DLLs necesarias).
- Icono `.ico` (se usa también para el acceso directo).
- Opcional: licencia (`LICENSE`, MIT) en la pantalla de bienvenida.
- Registro en "Agregar o quitar programas" (nombre, versión, editor, icono).

### 1.3 MSIX

Paquete moderno de Windows (instalación limpia, actualizaciones delta,
sandbox). Requiere **firma de código** obligatoria y un manifiesto
`AppxManifest.xml`.

- Herramientas: `MakeAppx.exe`, `SignTool.exe` (Windows SDK), o
  `cargo-msix`/`MSIX Packaging Tool`.
- Cuándo: cuando se quiera distribución por Microsoft Store o updates
  gestionados por el sistema. No es prioridad para la v1 (la firma es un
  prerequisito duro y caro de arrancar).

---

## 2. Firma de código

Necesaria para que Windows SmartScreen no muestre el aviso de "editor
desconocido" y para MSIX.

- **Certificado**: uno de firma de código (OV u EV) de una CA (DigiCert,
  Sectigo, SSL.com) o un certificado autofirmado para pruebas internas.
  - EV permite reputación inmediata en SmartScreen; OV acumula reputación con
    el tiempo. Para empezar, un certificado de prueba local sirve para
    validar el pipeline.
- **Herramienta**: `SignTool.exe sign /fd SHA256 /a /tr http://timestamp.digicert.com ...`.
- **Dónde firmar**: en CI, con el `.pfx` guardado como secreto (nunca en el
  repo). Firmar **tanto el `.exe` portable como el instalador** (Inno permite
  firmar el `Setup.exe` al final).
- **Windows will block**: sin firma, SmartScreen marca "Windows protected your
  PC" y el usuario tiene que pulsar "Más información → Ejecutar de todos
  modos". Con OV/EV firmado, desaparece con reputación.

---

## 3. Icono

Hoy el icono del tray se **genera en código** (círculo violeta RGBA 32×32,
`src/tray.rs`). Falta el icono de **archivo/ventana** del binario y del acceso
directo.

- Crear un `.ico` multi-resolución: 16, 24, 32, 48, 64, 128, 256 px (256
  obligatorio para Windows 11). Mantener el mismo lenguaje: círculo violeta
  `#7C6CFF` sobre fondo transparente.
- Incrustarlo como recurso de Windows vía `winres` (crate) o un archivo
  `.rc` + `windres`, apuntando al `.ico`. Añadir también el manifest de
  ejecución (DPI awareness, common controls).
- Reutilizar el mismo `.ico` en el instalador y en el acceso directo.

---

## 4. Versionado (SemVer)

- Esquema: `MAJOR.MINOR.PATCH`. Actual: **0.2.0** (`Cargo.toml`).
  - `0.x`: API inestable; cada feature nueva sube MINOR, los fixes PATCH.
  - Se pasa a `1.0.0` cuando la app tenga el flujo completo (corrección por
    voz incluida) y una estabilidad de formato demostrada.
- Fuente de verdad única: `Cargo.toml` `version`. Evitar duplicar la versión
  en el instalador: Inno puede leerla de una variable o de un fichero
  generado (p. ej. `cargo metadata`).
- Convención de commits (opcional): prefijos `feat:` / `fix:` / `docs:` /
  `chore:` (ya se usan en el repo) para derivar CHANGELOG y el bump.

---

## 5. Actualizaciones

Opciones, de menor a mayor esfuerzo:

1. **Nada automático (v1)**: el usuario baja el nuevo instalador a mano.
   Suficiente mientras la base de usuarios sea reducida.
2. **Comprobación de versión** (barato): al arrancar, consultar un endpoint
   estático (p. ej. la release más reciente de GitHub) y avisar "hay versión
   nueva" con enlace. Sin descarga automática.
3. **Auto-update completo**: 
   - Portable: `self_update`/`sparkle`-like (reemplazar el binario y
     relanzar). `self_replace` + un descargador.
   - Instalador Inno: Inno Download Plugin, o descargar el `Setup.exe` y
     relanzar.
   - MSIX: actualización nativa del sistema.

Recomendación: 2 → 3 en ese orden. El backend de releases de GitHub (tags +
assets del instalador) es el canal natural para v1.

---

## 6. Paquetes / ediciones futuras

- **Edición única (v1)**: una sola distribución local completa. La app es
  privada por diseño ("nada sale de tu PC"); una única edición refuerza el
  mensaje.
- **Ediciones opcionales futuras** (solo si hay demanda):
  - *Lite*: sin el LLM corrector embebido (usa fallback por reglas), binario
    más pequeño para equipos sin GPU.
  - *Pro*: notetaker de reuniones, identificación de hablantes, cloud
    opcional.
  - *Portable* vs *Instalada* no son "ediciones": son dos artefactos de la
    misma edición.
- Cualquier edición de pago introduciría licencia; hoy la app es MIT.

---

## 7. Pipeline de release (propuesta de CI)

1. `cargo test` + `cargo check` (Windows, toolchain MSVC — ver `AGENTS.md`).
2. `cargo build --release` + incrustar icono/manifest.
3. Firmar `.exe` con `SignTool`.
4. Generar instalador con Inno Setup, firmar `Setup.exe`.
5. Publicar assets en la release de GitHub con la etiqueta `v0.2.x` + CHANGELOG.

## 8. Criterios de aceptación

- Un `.exe` portable con icono propio que arranca sin consola.
- Un instalador que: instala en `%LOCALAPPDATA%`, crea accesos directos,
  registra desinstalador y muestra versión correcta.
- Ambos firmados (o al menos el pipeline de firma validado con certificado de
  prueba).
- La versión en todos los artefactos coincide con `Cargo.toml`.
