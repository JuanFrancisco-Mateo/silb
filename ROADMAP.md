# ROADMAP — Silb

> **La hoja de ruta al día de este proyecto es [`docs/ROADMAP.md`](docs/ROADMAP.md).**
> Este fichero es solo la puerta de entrada.

Silb es una app de **dictado por voz local y de escritorio nativo**
(Rust + Iced + whisper.cpp). Rama activa: `native-iced`. La rama `main` guarda
la versión web antigua (React/TS + Tauri) y desde aquí no se toca; su plan
original quedó archivado en
[`docs/ROADMAP-LEGADO-WEB.md`](docs/ROADMAP-LEGADO-WEB.md).

## Dónde está cada cosa

| Qué buscas | Dónde |
|---|---|
| Roadmap vivo: estado + próximas etapas priorizadas | [`docs/ROADMAP.md`](docs/ROADMAP.md) |
| Estado de las fases del encargo «cerrar el producto» | [`NOTA-SESION.md`](NOTA-SESION.md) |
| Reglas de entorno y definición de «done» | [`AGENTS.md`](AGENTS.md) |
| El código, fichero por fichero y el viaje de una frase | [`docs/ARQUITECTURA.md`](docs/ARQUITECTURA.md) |
| Guía de uso (sin jerga técnica) | [`docs/GUIA-USUARIO.md`](docs/GUIA-USUARIO.md) |
| Corrección por voz con IA: diseño y proveedores | [`docs/CORRECCION-VOZ.md`](docs/CORRECCION-VOZ.md) · [`docs/SETUP.md`](docs/SETUP.md) |
| Instalador, icono, portable y firma | [`docs/EMPAQUETADO.md`](docs/EMPAQUETADO.md) |
| Mejoras de UI pendientes | [`docs/UI-FUTURA.md`](docs/UI-FUTURA.md) |
| Roadmap de la versión web (histórico, rama `main`) | [`docs/ROADMAP-LEGADO-WEB.md`](docs/ROADMAP-LEGADO-WEB.md) |

## Resumen de un vistazo (26/10/2026)

- Versión: **0.5.0** (port a Linux integrado; la 0.4.0 cerró el producto para
  Windows).
- Fases del encargo de cierre: **0–3 hechas** (resistencia a fallos; nota
  editable y accesibilidad; código abierto con licencia MIT y documentación;
  comentarios de documentación en español en las funciones públicas). **Fase 4
  pendiente**: instalador, icono `.ico`, versión portable y medidas de ligereza.
  Fase 5: recorrido completo de verdad + informe de cierre.
- Comprobación de hoy: `cargo.exe test` → **336 passed, 0 failed, 3 ignored**;
  `cargo.exe check` sin errores. Los números actualizados viven en
  [`README.md`](README.md#tests) y [`docs/ROADMAP.md`](docs/ROADMAP.md).

## Definición de «done» (heredada de `AGENTS.md`)

1. `cmd.exe /c "cd /d whispr-open && cargo test"` → todo verde.
2. `... cargo.exe check` → sin errores (avisos tolerados).
3. Un commit por fase lógica, con el mensaje en español.
