//! Fuentes de la UI: **Inter** (embebida en el binario, la fuente del diseño
//! de Figma) y una fuente emoji (Segoe UI Emoji en Windows) para que los
//! símbolos de la interfaz (✓ ☀ ☾ ▾ ▸ ● → ✕ ⚠ ↩) no se vean como cajas vacías.
//!
//! # Por qué Inter embebida (y no la fuente del sistema)
//!
//! El rediseño (`design/openwhispr-figma/`) especifica **Inter** (Regular 400,
//! Medium 500, SemiBold 600). La app pedía pesos (`Weight::Medium`) sobre
//! `Family::SansSerif`, que en Windows significa `Segoe UI`; como `Segoe UI`
//! **no tiene variante Medium**, `cosmic-text` descartaba la familia entera y
//! elegía una fuente arbitraria del sistema con peso 500 (a menudo serif o
//! simbólica, sin tildes), apareciendo glifos `.notdef` (cajas `□`) en
//! «Micrófono» o «Idioma de transcripción». Embeber Inter con sus tres pesos
//! DA el peso pedido, el mismo en cualquier equipo, y elimina el fallback.
//!
//! # Cómo se activa
//!
//! - `main.rs` carga los tres `.ttf` al arranque (`iced_winit::Settings.fonts`)
//!   y fija `graphics::Settings.default_font = fonts::regular()`: TODO texto
//!   sin `.font()` explícito (y los `pick_list`, `text_input`, `text_editor`,
//!   menús…) sale ya en Inter Regular.
//! - Los textos que el diseño quiere en Medium/SemiBold piden
//!   [`medium`]/[`semibold`] explícitamente.
//! - Los textos con símbolos usan `Shaping::Advanced` (ver `ui_text` en
//!   `app.rs`): así el fallback por codepoint alcanza Segoe UI Emoji.

use iced::font::Weight;
use iced::Font;

/// Familia de la fuente emoji por defecto (Windows: Segoe UI Emoji).
pub const EMOJI_FAMILY: &str = "Segoe UI Emoji";

/// Familia de la fuente de marca (Inter), embebida en el binario.
pub const INTER_FAMILY: &str = "Inter";

/// Inter Regular (400): texto de cuerpo del diseño.
pub const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");
/// Inter Medium (500): rótulos, títulos de fila y acción principal.
pub const INTER_MEDIUM: &[u8] = include_bytes!("../assets/fonts/Inter-Medium.ttf");
/// Inter SemiBold (600): títulos de página y wordmark.
pub const INTER_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");

/// Inter Regular como [`Font`] de Iced. Es el `default_font` de la app.
pub fn regular() -> Font {
    Font::with_name(INTER_FAMILY)
}

/// Inter Medium (peso 500) de la familia de marca.
pub fn medium() -> Font {
    Font {
        weight: Weight::Medium,
        ..regular()
    }
}

/// Inter SemiBold (peso 600) de la familia de marca.
pub fn semibold() -> Font {
    Font {
        weight: Weight::Semibold,
        ..regular()
    }
}

/// La fuente emoji como [`Font`] de Iced, para asignarla explícitamente a un
/// texto si se necesita. El resto de la UI la usa como fallback automático
/// (cosmic-text resuelve el glifo faltante contra esta familia). Solo es
/// relevante con `Shaping::Advanced` (ver `ui_text`).
pub fn emoji_font() -> Font {
    Font::with_name(EMOJI_FAMILY)
}

/// Las tres variantes de Inter, para el arranque (`iced_winit::Settings.fonts`).
pub fn inter_font_sources() -> Vec<std::borrow::Cow<'static, [u8]>> {
    vec![
        std::borrow::Cow::Borrowed(INTER_REGULAR),
        std::borrow::Cow::Borrowed(INTER_MEDIUM),
        std::borrow::Cow::Borrowed(INTER_SEMIBOLD),
    ]
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
/// glifos que deben renderizar sí o sí. Los tests lo recorren.
pub const UI_EMOJI: &[&str] = &[
    "✓", // confirmación: inyección / copia al portapapeles
    "☀", "☾", // toggle de tema (claro / oscuro)
    "▾", "▸", // botón de ajustes (abierto / cerrado)
    "●", // punto de estado del micrófono en el hero
    "→", // flecha de las entradas del diccionario (reemplazo)
    "✕", // botón de quitar entrada del diccionario
    "⚠", // aviso de contraste de la paleta
    "↩", // deshacer
];

/// Frases REALES de la UI con los caracteres conflictivos (tildes, eñe,
/// comillas tipográficas y «…»). Los tests comprueban que Inter los resuelve
/// en los tres pesos sin cajas; son exactamente los textos que se veían rotos.
pub const UI_ACCENTED_PHRASES: &[&str] = &[
    "Micrófono",
    "Idioma de transcripción",
    "Corrección IA",
    "Corrección IA (avanzado)",
    "Espera máx. (s)",
    "Sensibilidad: 50",
    "Modelo de voz",
    "El micrófono se desconectó a mitad: se transcribió lo que se oyó hasta ese momento.",
    "¿Quieres borrar también tu configuración personal y los modelos descargados?",
    "Atajo de dictado · sin conexión · señales · rápidamente",
];

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight as CosmicWeight};

    /// Da forma al texto con el mismo `FontSystem` que usa Iced (fuentes del
    /// sistema + cargas explícitas) y devuelve los glifos faltantes: los que
    /// quedaron en `glyph_id == 0` (`.notdef`), que es exactamente la "caja
    /// vacía" que reportó el usuario.
    fn missing_glyphs(font_system: &mut FontSystem, text: &str, attrs: Attrs) -> Vec<String> {
        let mut buffer = Buffer::new(font_system, Metrics::new(14.0, 18.0));
        buffer.set_size(font_system, Some(600.0), None);
        buffer.set_text(font_system, text, attrs, Shaping::Advanced);
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

    /// Simula la carga de arranque de Inter en el FontSystem (mismos bytes que
    /// embebe el binario por `include_bytes!`).
    fn load_inter(font_system: &mut FontSystem) {
        for bytes in [INTER_REGULAR, INTER_MEDIUM, INTER_SEMIBOLD] {
            font_system
                .db_mut()
                .load_font_source(cosmic_text::fontdb::Source::Binary(std::sync::Arc::new(
                    bytes,
                )));
        }
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
    fn assert_shapes(font_system: &mut FontSystem, text: &str, attrs: Attrs) {
        let missing = missing_glyphs(font_system, text, attrs);
        assert!(
            missing.is_empty(),
            "'{text}' con {attrs:?} tiene glifos sin resolver: {missing:?}"
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

    #[test]
    fn inter_es_la_familia_de_marca_en_los_tres_pesos() {
        assert_eq!(regular().family, iced::font::Family::Name(INTER_FAMILY));
        assert_eq!(regular().weight, Weight::Normal);
        assert_eq!(medium().family, iced::font::Family::Name(INTER_FAMILY));
        assert_eq!(medium().weight, Weight::Medium);
        assert_eq!(semibold().family, iced::font::Family::Name(INTER_FAMILY));
        assert_eq!(semibold().weight, Weight::Semibold);
    }

    /// Las tres variantes se cargan de verdad (bytes no vacíos y con la firma
    /// TTF/OTF). Sin esto, un `include_bytes!` mal apuntado pasaría inadvertido.
    #[test]
    fn las_tres_variantes_de_inter_estan_embebidas() {
        for (name, bytes) in [
            ("Regular", INTER_REGULAR),
            ("Medium", INTER_MEDIUM),
            ("SemiBold", INTER_SEMIBOLD),
        ] {
            assert!(bytes.len() > 100_000, "{name}: {} bytes", bytes.len());
            // Firma sfnt: 0x00010000 (TrueType) o 'OTTO' (CFF).
            let sig = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            assert!(
                sig == 0x0001_0000 || sig == u32::from_be_bytes(*b"OTTO"),
                "{name}: firma sfnt inesperada {sig:#010x}"
            );
        }
        assert_eq!(inter_font_sources().len(), 3);
    }

    /// El FontSystem de Iced expone la familia "Inter" con los tres pesos:
    /// si fontdb no los agrupa bajo el mismo nombre, el peso pedido no casa.
    #[test]
    fn fontdb_agrupa_los_tres_pesos_bajo_la_familia_inter() {
        let mut fs = system_fonts();
        load_inter(&mut fs);
        let mut pesos = Vec::new();
        for face in fs.db().faces() {
            if face.families.iter().any(|(n, _)| n == INTER_FAMILY) {
                pesos.push(face.weight.0);
            }
        }
        for esperado in [400u16, 500, 600] {
            assert!(
                pesos.contains(&esperado),
                "falta el peso {esperado} de Inter (encontrados {pesos:?})"
            );
        }
    }

    /// Los textos con tildes/eñe del diseño resuelven en Inter (los tres
    /// pesos) sin cajas: era EL síntoma reportado («Micr□fono»).
    #[test]
    fn las_frases_con_tildes_resuelven_en_inter_sin_cajas() {
        let mut fs = system_fonts();
        load_inter(&mut fs);
        for phrase in UI_ACCENTED_PHRASES {
            for weight in [
                CosmicWeight::NORMAL,
                CosmicWeight::MEDIUM,
                CosmicWeight::SEMIBOLD,
            ] {
                let attrs = Attrs::new()
                    .family(Family::Name(INTER_FAMILY))
                    .weight(weight);
                assert_shapes(&mut fs, phrase, attrs);
            }
        }
    }

    /// Todos los símbolos del inventario de la UI deben tener glifo una vez
    /// cargadas las fuentes (Inter + emoji). Con `Shaping::Advanced` el
    /// fallback por codepoint alcanza Segoe UI Emoji.
    #[test]
    fn inventario_ui_renderiza_con_fuentes_cargadas() {
        let mut fs = system_fonts();
        load_inter(&mut fs);
        if load_emoji(&mut fs).is_none() {
            eprintln!(
                "sin fuente emoji en este equipo (buscadas: {:?}); test no aplicable",
                emoji_font_candidates()
            );
            return;
        }
        for s in UI_EMOJI {
            assert_shapes(&mut fs, s, Attrs::new().family(Family::Name(INTER_FAMILY)));
            assert_shapes(&mut fs, s, Attrs::new().family(Family::Name(EMOJI_FAMILY)));
        }
    }

    /// Las frases reales de la UI (con texto mixto) deben salir completas:
    /// con el fallback automático y con la fuente emoji explícita. Es lo que
    /// hace el renderer en cada frame.
    #[test]
    fn frases_reales_de_la_ui_renderizan_sin_glifos_faltantes() {
        let mut fs = system_fonts();
        load_inter(&mut fs);
        if load_emoji(&mut fs).is_none() {
            eprintln!("sin fuente emoji en este equipo; test no aplicable");
            return;
        }

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
            "⚠ Contraste 2.10:1 (texto sobre fondo, mínimo WCAG 3:1).",
        ];
        for s in phrases {
            assert_shapes(&mut fs, s, Attrs::new().family(Family::Name(INTER_FAMILY)));
            assert_shapes(&mut fs, s, Attrs::new().family(Family::Name(EMOJI_FAMILY)));
        }
    }

    /// Diagnóstico: qué fuente resuelve cada glifo del inventario. Ver
    /// `--nocapture` para el detalle.
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
                    Attrs::new().family(Family::Name(INTER_FAMILY)),
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
        report(&mut fs, "solo sistema (sin Inter)");
        load_inter(&mut fs);
        load_emoji(&mut fs);
        report(&mut fs, "tras cargar Inter + emoji");
    }
}
