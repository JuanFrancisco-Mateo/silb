# Documentación de Silb

Silb convierte tu voz en texto sin salir de tu ordenador. Aquí está todo lo que
hay escrito sobre cómo se usa, cómo está hecho por dentro y cómo se empaqueta.

## Para usarlo

- [Guía de usuario](GUIA-USUARIO.md) - instalar, dictar, atajo global, ajustes y
  problemas frecuentes.
- [SETUP](SETUP.md) - elegir y configurar el corrector con IA (local, propio o
  en la nube).

## Para entenderlo

- [Arquitectura](ARQUITECTURA.md) - recorrido del código para quien no ha tocado
  Rust: micrófono → whisper → limpieza → inyección.
- [Corrección por voz](CORRECCION-VOZ.md) - los tres modos de IA, los guards del
  texto dictado y las latencias medidas.
- [Diseño e ideas](UI-FUTURA.md) - lo que ya está y lo que queda en la interfaz.

## Para empaquetarlo

- [Empaquetado](EMPAQUETADO.md) - instalador online y offline, detección de la
  GPU, pack de CUDA y cómo se publica una release.
- [Roadmap](ROADMAP.md) - qué está hecho y qué viene después.

## Port a Linux

- [Dependencias](PORT-LINUX/DEPENDENCIAS.md)
- [Código: núcleo](PORT-LINUX/CODIGO-CORE.md) - [Código: interfaz](PORT-LINUX/CODIGO-UI.md)
- [Audio y runtime](PORT-LINUX/AUDIO-RUNTIME.md)
- [Scripts](PORT-LINUX/SCRIPTS.md)
- [Atajo global en Hyprland](PORT-LINUX/HOTKEY-HYPR.md)

---

El código está en [github.com/JuanFrancisco-Mateo/silb](https://github.com/JuanFrancisco-Mateo/silb)
y las descargas en [Releases](https://github.com/JuanFrancisco-Mateo/silb/releases).
