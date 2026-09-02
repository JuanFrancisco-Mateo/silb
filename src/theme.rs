//! Tokens de diseño 2026 para Whispr Open.
//!
//! Única fuente de verdad de la capa visual: colores, espaciado, radios y
//! tamaños de fuente. CERO literales mágicos en la UI (app.rs).
//! Principios: calm design, dark mode por defecto, un solo acento violeta,
//! sin blur/glassmorphism, sin animaciones decorativas, grid de 8 px.

use iced::border::Radius;
use iced::theme::Palette;
use iced::widget::{
    button, checkbox as check, container, pick_list as pick, progress_bar, rule, scrollable as scroll,
    text_input as input,
};
use iced::{Background, Border, Color, Shadow, Theme};

/// Color desde hex `0xRRGGBB` (const, evaluado en compilación).
macro_rules! rgb {
    ($r:expr, $g:expr, $b:expr) => {
        Color::from_rgb($r as f32 / 255.0, $g as f32 / 255.0, $b as f32 / 255.0)
    };
}

// ─────────────────────────────── Tokens: color ───────────────────────────────
// Paleta grafito (dark, por defecto). Acento violeta único #7C6CFF en ambos
// modos; ok/warn/err se oscurecen ligeramente en light para contraste AA.
pub const BG: Color = rgb!(0x17, 0x18, 0x1C); // grafito fondo
pub const SURFACE: Color = rgb!(0x1F, 0x20, 0x26); // tarjetas e inputs
pub const SURFACE_ALT: Color = rgb!(0x27, 0x28, 0x30); // hover, fondos secundarios
pub const TEXT: Color = rgb!(0xF7, 0xF6, 0xF3);
pub const DIM: Color = rgb!(0x9A, 0x9A, 0xA3);
pub const ACCENT: Color = rgb!(0x7C, 0x6C, 0xFF); // violeta único
pub const ACCENT_HOVER: Color = rgb!(0x8F, 0x82, 0xFF);
pub const OK: Color = rgb!(0x3F, 0xB9, 0x5F);
pub const WARN: Color = rgb!(0xF5, 0xA6, 0x23);
pub const ERR: Color = rgb!(0xE5, 0x4B, 0x4B);
pub const BORDER: Color = rgb!(0x2E, 0x30, 0x38);

pub const BG_LIGHT: Color = rgb!(0xF6, 0xF5, 0xF2);
pub const SURFACE_LIGHT: Color = rgb!(0xFF, 0xFF, 0xFF);
pub const SURFACE_ALT_LIGHT: Color = rgb!(0xEB, 0xEB, 0xEF);
pub const TEXT_LIGHT: Color = rgb!(0x17, 0x18, 0x1C);
pub const DIM_LIGHT: Color = rgb!(0x6B, 0x6B, 0x76);
pub const ACCENT_HOVER_LIGHT: Color = rgb!(0x66, 0x52, 0xF2);
pub const OK_LIGHT: Color = rgb!(0x2F, 0x9E, 0x53);
pub const WARN_LIGHT: Color = rgb!(0xD9, 0x8E, 0x0B);
pub const ERR_LIGHT: Color = rgb!(0xC9, 0x3E, 0x3E);
pub const BORDER_LIGHT: Color = rgb!(0xD9, 0xD9, 0xE0);

/// Paleta semántica resuelta por modo (dark/light).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiPalette {
    pub bg: Color,
    pub surface: Color,
    pub surface_alt: Color,
    pub text: Color,
    pub dim: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub ok: Color,
    pub warn: Color,
    pub err: Color,
    pub border: Color,
}

pub const DARK: UiPalette = UiPalette {
    bg: BG,
    surface: SURFACE,
    surface_alt: SURFACE_ALT,
    text: TEXT,
    dim: DIM,
    accent: ACCENT,
    accent_hover: ACCENT_HOVER,
    ok: OK,
    warn: WARN,
    err: ERR,
    border: BORDER,
};

pub const LIGHT: UiPalette = UiPalette {
    bg: BG_LIGHT,
    surface: SURFACE_LIGHT,
    surface_alt: SURFACE_ALT_LIGHT,
    text: TEXT_LIGHT,
    dim: DIM_LIGHT,
    accent: ACCENT, // acento único: mismo violeta en ambos modos
    accent_hover: ACCENT_HOVER_LIGHT,
    ok: OK_LIGHT,
    warn: WARN_LIGHT,
    err: ERR_LIGHT,
    border: BORDER_LIGHT,
};

// ─────────────────────────────── Tokens: geometría ───────────────────────────────
// Grid de 8 px.
pub const SPACE_1: u16 = 4; // medio paso
pub const SPACE_2: u16 = 8;
pub const SPACE_3: u16 = 16;
pub const SPACE_4: u16 = 24;
pub const SPACE_5: u16 = 32;
pub const SPACE_6: u16 = 48;

// Radios 8–10 px.
pub const RADIUS_SM: f32 = 8.0;
pub const RADIUS_MD: f32 = 10.0;

// Tamaños de fuente.
pub const FONT_XS: u16 = 11;
pub const FONT_SM: u16 = 12;
pub const FONT_MD: u16 = 14;
pub const FONT_LG: u16 = 16;
pub const FONT_XL: u16 = 20;
pub const FONT_HERO: u16 = 24; // label del estado del micrófono
// (FONT_DOT se eliminó: el punto de estado del hero ahora usa FONT_XL)

// ─────────────────────────────── Temas iced ───────────────────────────────

/// Tema custom para la app según el modo del `Config` (dark por defecto).
pub fn custom(dark: bool) -> Theme {
    if dark {
        Theme::custom(
            "Whispr Open — oscuro".to_string(),
            Palette {
                background: DARK.bg,
                text: DARK.text,
                primary: DARK.accent,
                success: DARK.ok,
                danger: DARK.err,
            },
        )
    } else {
        Theme::custom(
            "Whispr Open — claro".to_string(),
            Palette {
                background: LIGHT.bg,
                text: LIGHT.text,
                primary: LIGHT.accent,
                success: LIGHT.ok,
                danger: LIGHT.err,
            },
        )
    }
}

/// `true` si el tema activo es el oscuro (compara la paleta con la nuestra).
pub fn is_dark(theme: &Theme) -> bool {
    theme.palette().background == DARK.bg
}

/// Paleta semántica del tema activo.
pub fn tokens(theme: &Theme) -> &'static UiPalette {
    if is_dark(theme) { &DARK } else { &LIGHT }
}

fn border(color: Color, width: f32, radius: f32) -> Border {
    Border {
        color,
        width,
        radius: Radius::from(radius),
    }
}

/// Alpha absoluta (la API de iced 0.13 solo ofrece scale_alpha).
fn alpha(c: Color, a: f32) -> Color {
    Color {
        r: c.r,
        g: c.g,
        b: c.b,
        a,
    }
}

// ─────────────────────────────── Estilos de widgets ───────────────────────────────

/// Tarjeta base: superficie + borde sutil + radio medio.
pub fn card(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        text_color: Some(t.text),
        background: Some(Background::Color(t.surface)),
        border: border(t.border, 1.0, RADIUS_MD),
        shadow: Shadow::default(),
    }
}

/// Tarjeta sin borde (zonas planas, p. ej. el transcript).
pub fn flat(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        text_color: Some(t.text),
        background: Some(Background::Color(t.surface)),
        border: border(Color::TRANSPARENT, 0.0, RADIUS_MD),
        shadow: Shadow::default(),
    }
}

/// Botón primario (acción principal): fondo acento violeta, texto del color de fondo.
pub fn primary(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => t.accent_hover,
        _ => t.accent,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: t.bg,
        border: border(Color::TRANSPARENT, 0.0, RADIUS_SM),
        shadow: Shadow::default(),
    }
}

/// Botón secundario (acciones discretas): transparente, texto dim → texto al hover.
pub fn secondary(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let (bg, fg) = match status {
        button::Status::Hovered | button::Status::Pressed => (Some(t.surface_alt), t.text),
        _ => (None, t.dim),
    };
    button::Style {
        background: bg.map(Background::Color),
        text_color: fg,
        border: border(Color::TRANSPARENT, 0.0, RADIUS_SM),
        shadow: Shadow::default(),
    }
}

/// Botón del micrófono (hero): transparente; la tarjeta interna comunica el estado.
/// El cursor pointer del botón es el micro-feedback de hover.
pub fn mic(theme: &Theme, _status: button::Status) -> button::Style {
    let t = tokens(theme);
    button::Style {
        background: None,
        text_color: t.text,
        border: border(Color::TRANSPARENT, 0.0, RADIUS_MD),
        shadow: Shadow::default(),
    }
}

/// Tarjeta del estado del micrófono: borde con el color de estado (feedback doble:
/// color del borde + color del punto + label textual).
pub fn mic_card(theme: &Theme, state: Color) -> container::Style {
    let t = tokens(theme);
    container::Style {
        text_color: Some(t.text),
        background: Some(Background::Color(t.surface)),
        border: border(state, 1.0, RADIUS_MD),
        shadow: Shadow::default(),
    }
}

/// Campo de texto: fondo alt, borde acento al enfocar.
pub fn text_input(theme: &Theme, status: input::Status) -> input::Style {
    let t = tokens(theme);
    let border_color = match status {
        input::Status::Focused => t.accent,
        _ => t.border,
    };
    input::Style {
        background: Background::Color(t.surface_alt),
        border: border(border_color, 1.0, RADIUS_SM),
        icon: t.dim,
        placeholder: t.dim,
        value: t.text,
        selection: alpha(t.accent, 0.35),
    }
}

/// Selector desplegable: mismo lenguaje visual que el campo de texto.
pub fn pick_list(theme: &Theme, status: pick::Status) -> pick::Style {
    let t = tokens(theme);
    let border_color = match status {
        pick::Status::Opened | pick::Status::Hovered => t.accent,
        _ => t.border,
    };
    pick::Style {
        text_color: t.text,
        placeholder_color: t.dim,
        handle_color: t.dim,
        background: Background::Color(t.surface_alt),
        border: border(border_color, 1.0, RADIUS_SM),
    }
}

/// Checkbox: casilla alt, check en acento.
pub fn checkbox(theme: &Theme, status: check::Status) -> check::Style {
    let t = tokens(theme);
    let is_checked = match status {
        check::Status::Active { is_checked }
        | check::Status::Hovered { is_checked }
        | check::Status::Disabled { is_checked } => is_checked,
    };
    check::Style {
        background: Background::Color(if is_checked {
            alpha(t.accent, 0.18)
        } else {
            t.surface_alt
        }),
        icon_color: t.accent,
        border: border(
            if is_checked { t.accent } else { t.border },
            1.0,
            RADIUS_SM,
        ),
        text_color: Some(t.text),
    }
}

/// Área de scroll: contenedor plano y rieles discretos.
pub fn scrollable(theme: &Theme, _status: scroll::Status) -> scroll::Style {
    let t = tokens(theme);
    let rail = scroll::Rail {
        background: None,
        border: border(Color::TRANSPARENT, 0.0, RADIUS_SM),
        scroller: scroll::Scroller {
            color: alpha(t.dim, 0.4),
            border: border(Color::TRANSPARENT, 0.0, RADIUS_SM),
        },
    };
    scroll::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
    }
}

/// Separador horizontal sutil.
pub fn rule(theme: &Theme) -> rule::Style {
    rule::Style {
        color: tokens(theme).border,
        width: 1,
        radius: Radius::from(1.0),
        fill_mode: rule::FillMode::Full,
    }
}

/// Barra de progreso con color específico (nivel de micrófono / descarga).
pub fn progress(theme: &Theme, color: Color) -> progress_bar::Style {
    let t = tokens(theme);
    progress_bar::Style {
        background: Background::Color(t.surface_alt),
        bar: Background::Color(color),
        border: border(Color::TRANSPARENT, 0.0, RADIUS_SM),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_sigue_la_grid_de_8px() {
        for s in [SPACE_2, SPACE_3, SPACE_4, SPACE_5, SPACE_6] {
            assert_eq!(s % 8, 0, "spacing {s} debe ser múltiplo de 8");
        }
    }

    #[test]
    fn radios_dentro_de_8_10px() {
        assert!(RADIUS_SM >= 8.0 && RADIUS_SM <= 10.0);
        assert!(RADIUS_MD >= 8.0 && RADIUS_MD <= 10.0);
    }

    #[test]
    fn paleta_dark_mandada_en_la_tarea() {
        assert_eq!(DARK.bg, BG);
        assert_eq!(DARK.surface, SURFACE);
        assert_eq!(DARK.text, TEXT);
        assert_eq!(DARK.dim, DIM);
        assert_eq!(DARK.accent, ACCENT, "acento violeta único");
        assert_eq!(DARK.ok, OK);
        assert_eq!(DARK.warn, WARN);
        assert_eq!(DARK.err, ERR);
    }

    #[test]
    fn acento_es_el_mismo_en_ambos_modos() {
        assert_eq!(DARK.accent, LIGHT.accent);
    }

    #[test]
    fn tokens_resuelven_por_modo() {
        let dark_theme = custom(true);
        assert!(is_dark(&dark_theme));
        assert_eq!(tokens(&dark_theme), &DARK);
        let light_theme = custom(false);
        assert!(!is_dark(&light_theme));
        assert_eq!(tokens(&light_theme), &LIGHT);
    }
}