# Contribuir a Silb

¡Gracias por echar una mano! Silb es dictado por voz **en tu propio equipo** para
Windows (Rust + Iced + whisper.cpp), y cualquier ayuda cuenta: código,
traducciones, documentación, avisos de fallos o ideas.

## Cómo contribuir

- **🐛 Avisar de un fallo**: abre un issue con la
  [plantilla de fallo](https://github.com/JuanFrancisco-Mateo/whispr-open/issues/new?template=bug.yml).
  Cuenta qué versión de la app usas, qué versión de Windows tienes, los pasos
  para que vuelva a pasar y qué esperabas que ocurriera. Pega el registro
  (`%APPDATA%\whispr-open\whispr-open.log`) **sin tus datos personales**.
- **💡 Pedir una función**: usa la
  [plantilla de función](https://github.com/JuanFrancisco-Mateo/whispr-open/issues/new?template=feature.yml).
  Cuenta primero el problema que quieres resolver; la solución ya la
  discutimos entre todos.
- **🔧 Mandar código**: fork + rama + pull request (ver abajo).

## Ramas

| Rama | Qué es |
|---|---|
| `native-iced` | **Rama activa.** El producto actual (escritorio nativo). Las PRs van aquí. |
| `main` | Versión web antigua (React + Tauri). No se toca; se guarda como referencia. |
| `feature/port-linux-*` | Port **experimental** a Linux (los avisos de `cargo check` todavía se están resolviendo). Se agradece ayuda. |

## Entorno de desarrollo

- Windows 10/11.
- Rust con la herramienta de compilación MSVC (`rustup default stable-msvc`).
- Visual Studio Build Tools con **MSVC** y **CMake** (whisper-rs compila
  whisper.cpp, que está en C, en la primera compilación, y esa tarda 5–10 min).

```powershell
git clone https://github.com/<tu-usuario>/whispr-open.git
cd whispr-open
git checkout -b feature/mi-cambio origin/native-iced
cargo test          # antes de tocar nada: debe estar todo verde
cargo run --release
```

> Si trabajas desde WSL, llama siempre al cargo de Windows a través de
> `cmd.exe`; el cargo de WSL genera un binario para Linux que aquí no sirve.

## Antes de enviar la PR

1. `cargo test` → **todo verde** (336 passed; los ignorados son a propósito).
2. `cargo check` → sin errores (los avisos se toleran).
3. Comentarios de documentación en español en las funciones y tipos públicos.
4. Tests nuevos para la lógica de dominio (`src/domain/`) y para cualquier
   arreglo que se pueda reproducir sin hardware.
5. Nada de contraseñas, rutas personales ni datos privados en el código ni en la
   documentación.

## Estilo

- **Commits en español**, con el formato `tipo(ámbito): mensaje` — por ejemplo
  `fix(llm): el guard numérico caza el numeral viejo en frases naturales`.
  Explica el *porqué*, no el *qué*.
- Mejor cambios pequeños y fáciles de revisar; una PR = una idea.
- La interfaz se construye con Iced (sin navegador incrustado): no se meten
  HTML/CSS/JS ni frameworks web.
- La app tiene que poder usarse sin internet: todo lo que no sea la corrección
  con IA opcional debe funcionar 100% sin conexión.

## Dudas

Abre un issue con la etiqueta de discusión o pregunta ahí mismo; los issues
también valen como foro.

¡Gracias por leer esto hasta el final! 🎉
