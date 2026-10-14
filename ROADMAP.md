# ROADMAP — Silb

> **El roadmap vivo de este proyecto es [`docs/ROADMAP.md`](docs/ROADMAP.md).**
> Este fichero es solo el punto de entrada.

Silb es una app de **dictado por voz local y de escritorio nativo**
(Rust + Iced + whisper.cpp). Rama activa: `native-iced`. La rama `main`
conserva la versión web legada (React/TS + Tauri) y no se toca desde aquí; su
plan original quedó archivado en
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

## Resumen de un vistazo (11/09/2026)

- Versión: **0.2.0**.
- Fases del encargo de cierre: **0–3 hechas** (robustez; nota editable y
  accesibilidad; open source con licencia MIT y documentación; doc-comments de
  las funciones públicas en español). **Fase 4 pendiente**: instalador,
  icono `.ico`, variante portable y medidas de ligereza. Fase 5: flujo real
  completo + informe de cierre.
- Verificación de hoy: `cargo.exe test` → **271 passed, 0 failed, 1 ignored**;
  `cargo.exe check` sin errores. Los números actualizados viven en
  [`README.md`](README.md#tests) y [`docs/ROADMAP.md`](docs/ROADMAP.md).

## Definición de «done» (heredada de `AGENTS.md`)

1. `cmd.exe /c "cd /d E:\AI\FREEBUFF && C:\Users\usuario\.cargo\bin\cargo.exe test"` → todo verde.
2. `... cargo.exe check` → sin errores (warnings tolerados).
3. Commit por fase lógica, mensaje en español.
