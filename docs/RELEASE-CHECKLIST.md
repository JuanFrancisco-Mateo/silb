# Checklist para publicar la release v0.3.0

Guía paso a paso para sacar la primera release pública de Whispr Open con su
instalador. Cada casilla es una acción real; no marques nada que no se haya
hecho.

## 1. Antes de nada (una sola vez, al hacer público el repo)

- [ ] **Privacidad**: decidir qué hacer con `qa-screenshots*/` (capturas con
  contenido personal: webcam y página de terceros). Ojo: quitarlas del árbol
  NO las borra del historial; para purgarlas hace falta
  `git filter-repo` antes de hacer el push inicial público.
- [ ] **Rama por defecto**: en GitHub → Settings → Branches, poner
  `native-iced` como default (si no, los visitantes ven el README del legado
  web de `main`).
- [ ] **Vulnerability reporting**: activarlo en Settings → Code security.
- [ ] Revisar que no haya secretos en el historial (`git log -p | grep -i key`
  como salvaguarda; la API key vive en el Credential Manager, nunca en el repo).

## 2. Verificación del código

- [ ] `cargo test` → todo verde (320 passed; los ignored son intencionales).
- [ ] `cargo check` → sin errores.
- [ ] `cargo build --release` → `target\release\whispr-open.exe` actualizado.

## 3. Construir el instalador (Windows)

- [ ] Compilar el instalador con Inno Setup 6:

  ```cmd
  cmd /c ""%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe" /Q packaging\whispr-open.iss"
  ```

  (Versión con override: `ISCC.exe /DWHISPR_VERSION=0.3.0
  /DWHISPR_RELEASE_DIR=<dir> packaging\whispr-open.iss`.)

- [ ] Comprobar que sale `packaging\Output\whispr-open-0.3.0-setup.exe`
  (objetivo ≤25 MiB; el histórico ronda 23 MiB).
- [ ] Verificar el icono incrustado: `python scripts\verify-ico.py`.
- [ ] (Opcional) Medir RAM en reposo: `scripts\measure-ram.bat` (referencia:
  176 MB sin modelo).

## 4. Pruebas de humo del instalador

- [ ] Instalar en una máquina (o VM) limpia **sin permisos de admin**.
- [ ] `scripts\install-test.bat` y `scripts\check-arp.bat`: entrada correcta
  en «Agregar o quitar programas».
- [ ] `python scripts\install-smoke.py`: el binario instalado arranca.
- [ ] Primer arranque: descarga del modelo con progreso y chip de estado
  visible. Dictar una frase y comprobar la inyección en una app con foco.
- [ ] Desinstalar: pregunta por config/modelos y deja el ARP limpio.

## 5. Publicar

- [ ] Commit final del release + **tag anotada** `v0.3.0` (ya creada en local;
  `git push origin native-iced --follow-tags` cuando se autorice publicar).
- [ ] Crear la Release en GitHub:
  - Tag: `v0.3.0`, título: `Whispr Open 0.3.0`.
  - Notas: copiar la sección `[0.3.0]` de [`CHANGELOG.md`](../CHANGELOG.md).
  - Artefactos: `whispr-open-0.3.0-setup.exe` y `whispr-open.exe` (portable).
  - Marcar como **latest release**.
- [ ] Descargar el instalador desde la Release recién publicada y repetir la
  prueba de humo §4 (lo que subes es lo que se instala).
- [ ] Actualizar el README si hace falta (enlace de descargas ya apunta a
  Releases).

## 6. Después de publicar

- [ ] Anunciar dónde proceda y abrir issues de seguimiento para lo pendiente
  (`docs/UI-FUTURA.md`, port Linux).
- [ ] Vigilar los primeros issues/PRs y responder a tiempo — las primeras
  contribuciones definen la comunidad.
