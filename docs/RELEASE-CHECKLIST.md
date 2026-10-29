# Checklist para publicar la release v0.4.0

Estado a 2026-10-19. Solo se marca lo que está **hecho y comprobado**; lo que
depende del mantenedor queda sin marcar con su nota. Toda la evidencia está en
[`VERIFICACION-2026-10-19.txt`](VERIFICACION-2026-10-19.txt).

## 1. Antes de nada (una sola vez, al hacer público el repo)

- [x] **Privacidad**: `qa-screenshots*/` borrado con `git filter-repo` en un clon
  espejo; en el historial quedan 0. **El borrado NO está aplicado al remoto**:
  falta decidir entre reescribir `whispr-open` o publicar un repo nuevo (§7).
- [x] **Rama por defecto**: `native-iced` ya es la default en GitHub (comprobado
  por API el 19/10/2026).
- [ ] **Aviso de vulnerabilidades**: activarlo en Settings → Code security. Solo
  puede hacerlo el mantenedor.
- [x] **Sin contraseñas ni claves en el historial**: búsqueda de `sk-`, `ghp_`,
  `AIza` y `api_key=` sobre el clon limpio, sin resultados. La API key vive en
  el Administrador de credenciales, nunca en el repo.

## 2. Verificación del código

- [x] `cargo test` → **336 passed, 0 failed, 3 ignored** (19/10/2026).
- [x] `cargo check` → sin errores.
- [x] `cargo build --release` → `target/release/silb.exe` (22.410.240 B, CPU).
- [x] `cargo build --release --features cuda` → 48.581.120 B, con
  `cudart64_12` / `cublas64_12` / `ggml_cuda` dentro.

## 3. Construir el instalador (Windows)

- [x] Compilar el instalador con Inno Setup 6:

  ```cmd
  ISCC.exe /DWHISPR_VERSION=0.4.0 packaging/whispr-open.iss
  ```

- [x] Comprobar que sale `packaging/Output/silb-0.4.0-setup.exe`:
  **15.110.669 B (14,41 MiB)**, objetivo ≤25 MiB cumplido.
  sha256 `b00b5c3370fad844e7e10fbee4e4a43bf987716fdbdb7dc32b507def4c6ed5da`.
- [x] Comprobar el icono incrustado: `python scripts/verify-ico.py assets/whispr-open.ico`
  → OK, 7 entradas (16…256).
- [ ] (Opcional) Medir la RAM en reposo: `scripts/measure-ram.bat` (referencia
  antigua: 176 MB sin modelo). No medido en esta pasada.

## 4. Pruebas de humo del instalador

- [x] Instalar **sin permisos de admin** (por usuario, ARP `_is1`): exit 0.
- [x] ARP correcto: `DisplayName=Silb`, `DisplayVersion=0.4.0`,
  `InstallLocation=%LOCALAPPDATA%\Programs\Silb`, `DisplayIcon=silb.exe`,
  `InstallDate=20261019`, `EstimatedSize=71.690 KB`. Accesos en Inicio y
  Escritorio.
- [ ] `python scripts/install-smoke.py` arranca el binario instalado: el script
  comprueba ficheros y ARP, **no lanza la app**. Arranque real pendiente.
- [ ] Primer arranque, descarga de modelo, dictar una frase y que se escriba en
  otra app: **no hecho** (hace falta una persona, un micrófono y una ventana con
  el foco).
- [x] Desinstalar: exit 0, carpeta borrada y ARP limpio.

## 5. Publicar (nada de esto sin autorización explícita)

- [x] Etiqueta anotada `v0.4.0` creada en **local**.
- [ ] `git push origin native-iced --follow-tags`.
- [ ] Crear la Release en GitHub:
  - Tag: `v0.4.0`, título: `Silb 0.4.0`.
  - Notas: copiar la sección `[0.4.0]` de [`CHANGELOG.md`](../CHANGELOG.md).
  - Artefactos: `silb-0.4.0-setup.exe` y `silb.exe` (portable).
  - Marcar como **latest release**.
- [ ] Descargar el instalador de la Release publicada y repetir las pruebas
  rápidas §4.

## 6. Después de publicar

- [ ] Anunciarlo donde toque y abrir issues de seguimiento (`docs/UI-FUTURA.md`,
  port Linux).
- [ ] Vigilar los primeros issues y PRs — las primeras contribuciones son las que
  definen la comunidad.

## 7. Decisión pendiente: cómo publicar el historial

- **Opción A — reescribir `whispr-open`**: `git push --force --all` del clon
  limpio. Cambia todos los hashes, obliga a volver a clonar a quien lo tuviera y
  reescribe las 15 ramas (incluidas las 10 `feature/port-linux-*`).
  Coste: alto, y en la práctica no se puede deshacer.
- **Opción B — repo nuevo `silb`**: subir el clon limpio a un repositorio nuevo y
  dejar `whispr-open` en privado como archivo. No rompe nada, permite conservar
  el historial en privado y publicar solo lo limpio. **Recomendada.**
