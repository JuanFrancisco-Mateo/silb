//! Fuentes de la UI: carga de una fuente con glifos emoji (Segoe UI Emoji en
//! Windows) para que los símbolos de la interfaz (✓ ☀ ☾ ▾ ▸ ● → ✕) no se
//! vean como cajas vacías o glifos invisibles.
//!
//! Contexto (Iced 0.13 / cosmic-text 0.12): el `FontSystem` global de iced
//! arranca con las fuentes del sistema (`fontdb::load_system_fonts`) + la
//! fuente de iconos de Iced, y los textos con glifos que la fuente principal
//! no tiene se resuelven con el fallback por codepoint de cosmic-text (en
//! Windows la cadena común termina en "Segoe UI Emoji"). Para garantizar la
//! presencia de esa familia (aunque la enumeración del sistema falle o falte
//! en builds con otros toolchains), la cargamos explícitamente al arrancar
//! con `iced::font::load`, que añade los bytes al mismo `FontSystem` que usa
//! el renderer y fuerza el re-shape de los textos en el siguiente frame.

use iced::Font;

/// Familia de la fuente emoji por defecto (Windows: Segoe UI Emoji).
pub const EMOJI_FAMILY: &str = "Segoe UI Emoji";

/// La fuente emoji como [`Font`] de Iced, para asignarla explícitamente a un
/// texto si se necesita. El resto de la UI la usa como fallback automático
/// (cosmic-text resuelve el glifo faltante contra esta familia).
pub fn emoji_font() -> Font {
    Font::with_name(EMOJI_FAMILY)
}

/// Archivos candidatos a fuente emoji, en orden de preferencia.
fn emoji_font_candidates() -> Vec<&'static str> {
    vec![
        // Windows: Segoe UI Emoji (presente desde Windows 8; la interfaz de
        // Silb es Windows-first).
        r"C:\Windows\Fonts\seguiemj.ttf",
        r"C:\Windows\Fonts\Segoe UI Emoji.ttf",
        // Otras plataformas (desarrollo/CI): mejores aproximaciones.
        "/System/Library/Fonts/Apple Color Emoji.ttc",
        "/usr/share/fonts/noto/NotoColorEmoji.ttf",
        "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
        "/usr/share/fonts/noto-color-emoji/NotoColorEmoji.ttf",
    ]
}

/// Carga la primera fuente emoji disponible en el [`FontSystem`] de Iced.
///
/// Devuelve la [`iced::Task`] resultante para encadenarla al arranque, o
/// `None` si no hay ninguna fuente candidata (se avisa en el log; la UI
/// funciona igual, pero los símbolos pueden verse como cajas).
pub fn load_emoji_font() -> Option<iced::Task<Result<(), iced::font::Error>>> {
    for path in emoji_font_candidates() {
        if let Ok(bytes) = std::fs::read(path) {
            crate::log!(
                "fonts",
                "fuente emoji disponible: {path} ({} bytes)",
                bytes.len()
            );
            return Some(iced::font::load(bytes));
        }
        crate::log!("fonts", "fuente emoji no encontrada: {path}");
    }
    crate::log!(
        "fonts",
        "sin fuente emoji disponible; los símbolos de la UI pueden verse como cajas"
    );
    None
}

/// Inventario de símbolos que usa la UI actual (buscar en `src/`): son los
/// glifos que deben renderizar sí o sí. El test de shaping lo recorre.
pub const UI_EMOJI: &[&str] = &[
    "✓", // confirmación: inyección / copia al portapapeles
    "☀", "☾", // toggle de tema (claro / oscuro)
    "▾", "▸", // botón de ajustes (abierto / cerrado)
    "●", // punto de estado del micrófono en el hero
    "→", // flecha de las entradas del diccionario (reemplazo)
    "✕", // botón de quitar entrada del diccionario
];

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping};

    /// Da forma al texto con el mismo `FontSystem` que usa Iced (fuentes del
    /// sistema + cargas explícitas) y devuelve los glifos faltantes: los que
    /// quedaron en `glyph_id == 0` (`.notdef`), que es exactamente la "caja
    /// vacía" que reportó Juanfri.
    fn missing_glyphs(font_system: &mut FontSystem, text: &str, family: Family) -> Vec<String> {
        let mut buffer = Buffer::new(font_system, Metrics::new(14.0, 18.0));
        buffer.set_size(font_system, Some(600.0), None);
        buffer.set_text(
            font_system,
            text,
            Attrs::new().family(family),
            Shaping::Advanced,
        );
        buffer.shape_until_scroll(font_system, true);

        let mut missing = Vec::new();
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                if glyph.glyph_id == 0 {
                    missing.push(run.text[glyph.start..glyph.end].to_string());
                }
            }
        }
        missing
    }

    /// El FontSystem con el que arranca Iced en nativo: fuentes del sistema.
    fn system_fonts() -> FontSystem {
        FontSystem::new()
    }

    /// Simula `iced::font::load`: añade la fuente emoji al FontSystem.
    /// Devuelve la ruta usada, o `None` si este equipo no tiene ninguna.
    fn load_emoji(font_system: &mut FontSystem) -> Option<String> {
        for path in emoji_font_candidates() {
            if let Ok(bytes) = std::fs::read(path) {
                font_system
                    .db_mut()
                    .load_font_source(cosmic_text::fontdb::Source::Binary(std::sync::Arc::new(
                        bytes,
                    )));
                return Some(path.to_string());
            }
        }
        None
    }

    /// Helper: aserción "sin glifos faltantes" con mensaje útil.
    fn assert_shapes(font_system: &mut FontSystem, text: &str, family: Family) {
        let missing = missing_glyphs(font_system, text, family);
        assert!(
            missing.is_empty(),
            "'{text}' con familia {family:?} tiene glifos sin resolver: {missing:?}"
        );
    }

    #[test]
    fn candidatos_ponen_segue_ui_emoji_de_windows_primero() {
        let paths = emoji_font_candidates();
        assert_eq!(paths[0], r"C:\Windows\Fonts\seguiemj.ttf");
        assert!(paths.iter().any(|p| p.contains("Segoe UI Emoji")));
    }

    #[test]
    fn emoji_font_usa_la_familia_segue_ui_emoji() {
        assert_eq!(emoji_font().family, iced::font::Family::Name(EMOJI_FAMILY));
    }

    /// Todos los símbolos del inventario de la UI deben tener glifo una vez
    /// cargada la fuente emoji (familia por defecto + familia explícita).
    #[test]
    fn inventario_ui_renderiza_con_fuente_emoji_cargada() {
        let mut fs = system_fonts();
        let Some(path) = load_emoji(&mut fs) else {
            eprintln!(
                "sin fuente emoji en este equipo (buscadas: {:?}); test no aplicable",
                emoji_font_candidates()
            );
            return;
        };
        eprintln!("fuente emoji de prueba: {path}");

        for s in UI_EMOJI {
            assert_shapes(&mut fs, s, Family::SansSerif);
            assert_shapes(&mut fs, s, Family::Name(EMOJI_FAMILY));
        }
    }

    /// Las frases reales de la UI (con texto mixto) deben salir completas:
    /// con el fallback automático y con la fuente emoji explícita (que tiene
    /// latino como refuerzo). Es lo que hace el renderer en cada frame.
    #[test]
    fn frases_reales_de_la_ui_renderizan_sin_glifos_faltantes() {
        let mut fs = system_fonts();
        let Some(_path) = load_emoji(&mut fs) else {
            eprintln!("sin fuente emoji en este equipo; test no aplicable");
            return;
        };

        let phrases = [
            "Texto escrito en la app activa ✓",
            "Transcript copiado al portapapeles ✓",
            "☀ Claro",
            "☾ Oscuro",
            "Ajustes ▾",
            "Ajustes ▸",
            "↩ Deshacer",
            "Corrección IA deshecha ↩",
            "hablar → dictar",
        ];
        for s in phrases {
            assert_shapes(&mut fs, s, Family::SansSerif);
            assert_shapes(&mut fs, s, Family::Name(EMOJI_FAMILY));
        }
    }

    /// Línea base: sin cargar nada extra, ¿resuelve el fallback del sistema?
    /// Documenta el comportamiento real de la máquina donde corre la app.
    /// Si este test falla en campo, el fallback no alcanza y hace falta
    /// asignar `emoji_font()` explícitamente a los textos con símbolos.
    /// Diagnóstico: qué fuente resuelve cada glifo del inventario (antes y
    /// después de cargar la fuente emoji). Ver `--nocapture` para el detalle.
    #[test]
    fn diagnostico_fuente_por_glifo() {
        fn report(fs: &mut FontSystem, label: &str) {
            eprintln!("── {label} ──");
            for s in UI_EMOJI {
                let mut buffer = Buffer::new(fs, Metrics::new(14.0, 18.0));
                buffer.set_size(fs, Some(600.0), None);
                buffer.set_text(
                    fs,
                    s,
                    Attrs::new().family(Family::SansSerif),
                    Shaping::Advanced,
                );
                buffer.shape_until_scroll(fs, true);
                for run in buffer.layout_runs() {
                    for g in run.glyphs {
                        let name = fs
                            .db()
                            .face(g.font_id)
                            .and_then(|f| f.families.first())
                            .map(|(n, _)| n.clone())
                            .unwrap_or_default();
                        eprintln!(
                            "  U+{:04X} {s:?} → glyph {} en '{name}'",
                            run.text[g.start..g.end]
                                .chars()
                                .next()
                                .map(|c| c as u32)
                                .unwrap_or(0),
                            g.glyph_id
                        );
                    }
                }
            }
        }

        let mut fs = system_fonts();
        report(&mut fs, "solo sistema");
        load_emoji(&mut fs);
        report(&mut fs, "tras cargar seguiemj");
    }

    #[test]
    fn lineabase_sin_carga_explicita() {
        let mut fs = system_fonts();

        let missing = missing_glyphs(&mut fs, &UI_EMOJI.join(""), Family::SansSerif);
        if missing.is_empty() {
            eprintln!("fallback del sistema ya resuelve todos los símbolos de la UI");
        } else {
            eprintln!(
                "sin carga explícita faltan glifos: {missing:?} (el load de arranque los cubre)"
            );
        }
        // No assert: es diagnóstico. El contrato lo fijan los tests de arriba.
    }
}
