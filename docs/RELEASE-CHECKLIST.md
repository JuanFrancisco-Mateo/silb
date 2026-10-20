# Checklist para publicar la release v0.4.0

Estado a 2026-10-19. Solo se marca lo **ejecutado y verificado**; lo que depende
de juanfri queda desmarcado con su nota. Evidencia completa:
[`VERIFICACION-2026-10-19.txt`](VERIFICACION-2026-10-19.txt).

## 1. Antes de nada (una sola vez, al hacer público el repo)

- [x] **Privacidad**: `qa-screenshots*/` purgado con `git filter-repo` en un clon
  espejo; conteo en el historial = 0. **La purga NO está aplicada al remoto**:
  falta decidir entre reescribir `whispr-open` o publicar un repo nuevo (§7).
- [x] **Rama por defecto**: `native-iced` ya es la default en GitHub (verificado
  por API el 19/10/2026).
- [ ] **Vulnerability reporting**: activarlo en Settings → Code security. Solo
  puede hacerlo juanfri.
- [x] **Sin secretos en el historial**: búsqueda de `sk-`, `ghp_`, `AIza` y
  `api_key=` sobre el clon purgado, sin hallazgos. La API key vive en el
  Credential Manager, nunca en el repo.

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
- [x] Verificar el icono incrustado: `python scripts/verify-ico.py assets/whispr-open.ico`
  → OK, 7 entradas (16…256).
- [ ] (Opcional) Medir RAM en reposo: `scripts/measure-ram.bat` (referencia
  histórica: 176 MB sin modelo). No medido en esta pasada.

## 4. Pruebas de humo del instalador

- [x] Instalar **sin permisos de admin** (per-user, ARP `_is1`): exit 0.
- [x] ARP correcto: `DisplayName=Silb`, `DisplayVersion=0.4.0`,
  `InstallLocation=%LOCALAPPDATA%\Programs\Silb`, `DisplayIcon=silb.exe`,
  `InstallDate=20261019`, `EstimatedSize=71.690 KB`. Accesos en Inicio y
  Escritorio.
- [ ] `python scripts/install-smoke.py` arranca el binario instalado: el script
  verifica ficheros y ARP, **no lanza la app**. Arranque real pendiente.
- [ ] Primer arranque, descarga de modelo, dictado de una frase e inyección en
  otra app: **no hecho** (requiere persona, micrófono y foco).
- [x] Desinstalar: exit 0, directorio eliminado y ARP limpio.

## 5. Publicar (nada de esto sin autorización explícita)

- [x] Tag anotada `v0.4.0` creada en **local**.
- [ ] `git push origin native-iced --follow-tags`.
- [ ] Crear la Release en GitHub:
  - Tag: `v0.4.0`, título: `Silb 0.4.0`.
  - Notas: copiar la sección `[0.4.0]` de [`CHANGELOG.md`](../CHANGELOG.md).
  - Artefactos: `silb-0.4.0-setup.exe` y `silb.exe` (portable).
  - Marcar como **latest release**.
- [ ] Descargar el instalador de la Release publicada y repetir la prueba de
  humo §4.

## 6. Después de publicar

- [ ] Anunciar donde proceda y abrir issues de seguimiento (`docs/UI-FUTURA.md`,
  port Linux).
- [ ] Vigilar los primeros issues/PRs — las primeras contribuciones definen la
  comunidad.

## 7. Decisión pendiente: cómo publicar el historial

- **Opción A — reescribir `whispr-open`**: `git push --force --all` del clon
  purgado. Cambia todos los hashes, obliga a re-clonar a cualquiera que lo
  tuviera y reescribe las 15 ramas (incluidas las 10 `feature/port-linux-*`).
  Coste: alto, irreversible en la práctica.
- **Opción B — repo nuevo `silb`**: push del clon purgado a un repositorio
  nuevo y dejar `whispr-open` privado como archivo. No rompe nada, permite
  conservar el histórico privado y publicar solo lo limpio. **Recomendada.**
