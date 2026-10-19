# Contribuir a Silb

¡Gracias por echar una mano! Silb es dictado por voz **local** para
Windows (Rust + Iced + whisper.cpp), y toda contribución es bienvenida: código,
traducciones, documentación, reportes de bugs o ideas.

## Cómo contribuir

- **🐛 Reportar un bug**: abre un issue con la
  [plantilla de bug](https://github.com/JuanFrancisco-Mateo/whispr-open/issues/new?template=bug.yml).
  Incluye versión de la app, versión de Windows, pasos para reproducirlo y lo
  que esperabas que ocurriera. Pega el log
  (`%APPDATA%\whispr-open\whispr-open.log`) **sin tus datos personales**.
- **💡 Pedir una función**: usa la
  [plantilla de función](https://github.com/JuanFrancisco-Mateo/whispr-open/issues/new?template=feature.yml).
  Cuenta primero el problema que quieres resolver; la solución ya la
  discutimos entre todos.
- **🔧 Enviar código**: fork + rama + pull request (ver abajo).

## Ramas

| Rama | Qué es |
|---|---|
| `native-iced` | **Rama activa.** El producto actual (escritorio nativo). Las PRs van aquí. |
| `main` | Versión web legada (React + Tauri). No se toca; se conserva como referencia. |
| `feature/port-linux-*` | Port **experimental** a Linux (avisos de `cargo check` todavía en curso). Ayuda bienvenida. |

## Entorno de desarrollo

- Windows 10/11.
- Rust con toolchain MSVC (`rustup default stable-msvc`).
- Visual Studio Build Tools con **MSVC** y **CMake** (whisper-rs compila
  whisper.cpp en C en el primer build, que tarda 5–10 min).

```powershell
git clone https://github.com/<tu-usuario>/whispr-open.git
cd whispr-open
git checkout -b feature/mi-cambio origin/native-iced
cargo test          # antes de tocar nada: debe estar todo verde
cargo run --release
```

> Si trabajas desde WSL, invoca siempre el cargo de Windows vía `cmd.exe`;
> el cargo de WSL genera un binario Linux que no sirve aquí.

## Antes de enviar la PR

1. `cargo test` → **todo verde** (336 passed; los ignored son intencionales).
2. `cargo check` → sin errores (los warnings se toleran).
3. Doc-comments en español en funciones y tipos públicos.
4. Tests nuevos para lógica de dominio (`src/domain/`) y para cualquier fix
   que se pueda reproducir sin hardware.
5. Sin secretos, rutas personales ni datos privados en el código ni la docs.

## Estilo

- **Commits en español**, en formato `tipo(ámbito): mensaje` — por ejemplo
  `fix(llm): el guard numérico caza el numeral viejo en frases naturales`.
  Explica el *porqué*, no el *qué*.
- Prefiere cambios pequeños y revisables; una PR = una idea.
- La UI se construye con Iced (sin webview): no se introducen HTML/CSS/JS ni
  frameworks web.
- Mantén la app usable sin red: todo lo que no sea la corrección IA opcional
  debe funcionar 100% offline.

## Dudas

Abre un issue con la etiqueta de discusión o pregunta directamente en él; los
issues también valen como foro.

¡Gracias por leer esto hasta el final! 🎉
