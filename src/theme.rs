//! Tokens de diseño 2026 para Whispr Open.
//!
//! Única fuente de verdad de la capa visual: colores, espaciado, radios y
//! tamaños de fuente. CERO literales mágicos en la UI (app.rs).
//! Principios: calm design, dark mode por defecto, un solo acento violeta,
//! sin blur/glassmorphism, sin animaciones decorativas, grid de 8 px.

use iced::border::Radius;
use iced::theme::Palette;
use iced::widget::{
    button, checkbox as check, container, pick_list as pick, progress_bar, rule,
    scrollable as scroll, text_editor as editor, text_input as input,
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
// Variante de acento SEGURA como color de texto normal: #7C6CFF sobre la
// tarjeta grafito queda en ~4.2:1 (por debajo del AA 4.5:1 de WCAG para
// texto pequeño). #8F82FF (y #6652F2 en claro) miden ≥ 4.5:1 sobre fondo Y
// tarjeta en ambos modos (tests en este módulo). El violeta de marca se
// reserva para rellenos, elementos grandes (≥ 3:1) y estados.
pub const ACCENT_TEXT: Color = rgb!(0x8F, 0x82, 0xFF);
pub const OK: Color = rgb!(0x3F, 0xB9, 0x5F);
pub const WARN: Color = rgb!(0xF5, 0xA6, 0x23);
// Rojo medido para AA sobre tarjeta oscura: #E54B4F quedaba en ~4.2:1; este
// pasa 4.5:1 sobre fondo y tarjeta (ver test de contraste).
pub const ERR: Color = rgb!(0xEB, 0x57, 0x57);
pub const BORDER: Color = rgb!(0x2E, 0x30, 0x38);

pub const BG_LIGHT: Color = rgb!(0xF6, 0xF5, 0xF2);
pub const SURFACE_LIGHT: Color = rgb!(0xFF, 0xFF, 0xFF);
pub const SURFACE_ALT_LIGHT: Color = rgb!(0xEB, 0xEB, 0xEF);
pub const TEXT_LIGHT: Color = rgb!(0x17, 0x18, 0x1C);
pub const DIM_LIGHT: Color = rgb!(0x6B, 0x6B, 0x76);
// Hover del acento en claro: más CLARO que el acento (a diferencia del
// oscuro, cuyo hover sube de luminancia). Motivo medido: la tinta de los
// botones primarios (#17181C) sobre un hover oscuro (#6652F2) daba 3.42:1
// (< AA); sobre #9A8CFF mide 6.4:1. El hover más claro es además el patrón
// natural en modo claro.
pub const ACCENT_HOVER_LIGHT: Color = rgb!(0x9A, 0x8C, 0xFF);
// Acento-texto en modo claro (mismo rol que ACCENT_TEXT en oscuro): mide
// ≥ 4.5:1 sobre fondo claro y sobre tarjeta blanca. NO es el hover: son
// requisitos de contraste distintos (texto sobre fondo vs tinta sobre
// relleno) y cada uno necesita su tono.
pub const ACCENT_TEXT_LIGHT: Color = rgb!(0x66, 0x52, 0xF2);
// Verde y ámbar oscurecidos para AA en claro: los de antes (#2F9E53,
// #D98E0B) quedaban en ~3.1:1 y ~2.7:1 sobre blanco (texto pequeño < 4.5).
pub const OK_LIGHT: Color = rgb!(0x1F, 0x7A, 0x3E);
pub const WARN_LIGHT: Color = rgb!(0x8F, 0x63, 0x00);
pub const ERR_LIGHT: Color = rgb!(0xC9, 0x3E, 0x3E);
pub const BORDER_LIGHT: Color = rgb!(0xD9, 0xD9, 0xE0);

/// Tinta para texto sobre un botón RELLENO de acento (p. ej. «Aplicar»):
/// un casi-negro grafito que mide ≥ 4.5:1 sobre #7C6CFF en ambos modos
/// (el fondo del modo NO se puede usar aquí: en claro daría ~3.5:1).
pub const INK_ON_ACCENT: Color = rgb!(0x17, 0x18, 0x1C);

// ─────────────────────── Contraste WCAG (medido, no estimado) ────────────────
// Fórmulas de WCAG 2.1: luminancia relativa + ratio (1..21). Públicas porque
// los tests de accesibilidad de la UI (R4) miden con ellas los pares REALES
// de tokens que usa la app; ajustar un color sin re-medir queda prohibido
// por los propios tests.

/// Luminancia relativa WCAG de un color opaco (0..1).
fn relative_luminance(c: Color) -> f64 {
    let channel = |v: f32| {
        let v = v.clamp(0.0, 1.0) as f64;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b)
}

/// Ratio de contraste WCAG (1..21). AA texto normal ≥ 4.5; texto grande y
/// componentes gráficos ≥ 3.
pub fn contrast_ratio(fg: Color, bg: Color) -> f64 {
    let l1 = relative_luminance(fg);
    let l2 = relative_luminance(bg);
    let (hi, lo) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
    (hi + 0.05) / (lo + 0.05)
}

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
    /// Acento apto para TEXTO normal (AA ≥ 4.5:1 sobre bg y surface).
    /// Usar `accent` para rellenos y elementos grandes (≥ 3:1).
    pub accent_text: Color,
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
    accent_text: ACCENT_TEXT,
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
    accent_text: ACCENT_TEXT_LIGHT,
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

/// Tamaños de fuente resueltos por el tamaño seleccionado en Ajustes
/// (`Config::font_size()`). Factor 1.0 = los `FONT_*` de baseline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontSizes {
    pub xxs: u16,
    pub sm: u16,
    pub md: u16,
    pub lg: u16,
    pub xl: u16,
    pub hero: u16,
}

/// Escala los tamaños base (`FONT_XS..FONT_HERO`) por un factor. Clamp final
/// 8..=64 px: un factor extremo (p. ej. 1.3 con hero=24 → 31) nunca produce
/// tamaños absurdos. Redondeo al entero más cercano (la UI pide u16).
pub fn font_sizes(factor: f32) -> FontSizes {
    let scale = |base: u16| ((base as f32 * factor).round() as u16).clamp(8, 64);
    FontSizes {
        xxs: scale(FONT_XS),
        sm: scale(FONT_SM),
        md: scale(FONT_MD),
        lg: scale(FONT_LG),
        xl: scale(FONT_XL),
        hero: scale(FONT_HERO),
    }
}

/// Auto-tema: ¿el sistema operativo está en modo oscuro? Lee
/// `AppsUseLightTheme` (REG_DWORD) de
/// `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize`,
/// que Windows mantiene en caliente al cambiar el tema (claro/dark toggle).
/// `true` = oscuro. Cualquier fallo de lectura cae a `true` (oscuro, el
/// default de la app) — la configuración nunca rompe el arranque.
#[cfg(target_os = "windows")]
pub fn system_dark_mode() -> bool {
    use winapi::um::winreg::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe {
        let subkey = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
        let name = wide("AppsUseLightTheme");
        let mut data: u32 = 0;
        let mut size: u32 = std::mem::size_of::<u32>() as u32;
        let mut ty: u32 = 0;
        let r = RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            &mut ty,
            &mut data as *mut u32 as *mut _,
            &mut size,
        );
        if r == 0 {
            data == 0
        } else {
            true
        }
    }
}

/// Stub fuera de Windows: no hay registro del sistema que consultar; se asume
/// oscuro, que es el tema por defecto de la app.
#[cfg(not(target_os = "windows"))]
pub fn system_dark_mode() -> bool {
    true
}

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
    if is_dark(theme) {
        &DARK
    } else {
        &LIGHT
    }
}

fn border(color: Color, width: f32, radius: f32) -> Border {
    Border {
        color,
        width,
        radius: Radius::from(radius),
    }
}

/// Alpha absoluta (la API de iced 0.13 solo ofrece scale_alpha). Pública:
/// la burbuja la usa para la opacidad configurable (slider 20–100 %).
pub fn with_alpha(c: Color, a: f32) -> Color {
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

/// Botón primario (acción principal): fondo acento violeta, texto con la
/// tinta medida para AA sobre el acento (ver `INK_ON_ACCENT`: usar el color
/// de fondo del modo dejaba el claro en ~3.5:1).
pub fn primary(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => t.accent_hover,
        _ => t.accent,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: INK_ON_ACCENT,
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

/// Campo de texto: fondo alt, borde acento al enfocar. El foco se dibuja
/// con 1.5 px de acento (requisito de accesibilidad: el foco de teclado
/// debe VERSR, no inferirse).
pub fn text_input(theme: &Theme, status: input::Status) -> input::Style {
    let t = tokens(theme);
    let (border_color, border_width) = match status {
        input::Status::Focused => (t.accent, 1.5),
        _ => (t.border, 1.0),
    };
    input::Style {
        background: Background::Color(t.surface_alt),
        border: border(border_color, border_width, RADIUS_SM),
        icon: t.dim,
        placeholder: t.dim,
        value: t.text,
        selection: with_alpha(t.accent, 0.35),
    }
}

/// Editor multilínea del transcript (requisito R3): mismo lenguaje visual
/// que el campo de texto, con anillo de foco visible (1.5 px de acento).
pub fn text_editor(theme: &Theme, status: editor::Status) -> editor::Style {
    let t = tokens(theme);
    let (border_color, border_width) = match status {
        editor::Status::Focused => (t.accent, 1.5),
        _ => (t.border, 1.0),
    };
    editor::Style {
        background: Background::Color(t.surface),
        border: border(border_color, border_width, RADIUS_MD),
        icon: t.dim,
        placeholder: t.dim,
        value: t.text,
        selection: with_alpha(t.accent, 0.35),
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
            with_alpha(t.accent, 0.18)
        } else {
            t.surface_alt
        }),
        icon_color: t.accent,
        border: border(if is_checked { t.accent } else { t.border }, 1.0, RADIUS_SM),
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
            color: with_alpha(t.dim, 0.4),
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

    #[test]
    fn font_sizes_factor_1_es_el_baseline() {
        let fs = font_sizes(1.0);
        assert_eq!(fs.xxs, FONT_XS);
        assert_eq!(fs.sm, FONT_SM);
        assert_eq!(fs.md, FONT_MD);
        assert_eq!(fs.lg, FONT_LG);
        assert_eq!(fs.xl, FONT_XL);
        assert_eq!(fs.hero, FONT_HERO);
    }

    #[test]
    fn font_sizes_escala_y_mantiene_jerarquia() {
        // 0.85 (pequeña): todo más pequeño pero el orden se conserva.
        let small = font_sizes(0.85);
        assert!(small.sm < FONT_SM);
        assert!(small.md < FONT_MD);
        // 1.3 (muy grande): todo más grande.
        let large = font_sizes(1.3);
        assert!(large.md > FONT_MD);
        assert!(large.hero > FONT_HERO);
        // Jerarquía: cada nivel supera al anterior en cualquier factor.
        for f in [0.85, 1.0, 1.15, 1.3] {
            let fs = font_sizes(f);
            assert!(
                fs.xxs < fs.sm
                    && fs.sm < fs.md
                    && fs.md < fs.lg
                    && fs.lg < fs.xl
                    && fs.xl < fs.hero,
                "jerarquía rota con factor {f}: {fs:?}"
            );
        }
    }

    #[test]
    fn font_sizes_clamp_evita_absurdos() {
        // Factores extremos nunca salen del rango 8..=64 px.
        for f in [0.0, 0.5, 2.0, 10.0] {
            let fs = font_sizes(f);
            for v in [fs.xxs, fs.sm, fs.md, fs.lg, fs.xl, fs.hero] {
                assert!((8..=64).contains(&v), "fuera de rango con factor {f}: {v}");
            }
        }
    }

    #[test]
    fn with_alpha_cambia_solo_el_canal_alpha() {
        assert_eq!(
            with_alpha(Color::WHITE, 0.5),
            Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 0.5
            }
        );
        assert_eq!(with_alpha(BG, 0.2).a, 0.2);
        assert_eq!(with_alpha(BG, 0.2).r, BG.r, "RGB intacto");
    }

    // ── Contraste WCAG medido (requisito R4: medido, no estimado) ─────────
    //
    // Se miden los pares REALES de la UI: cada color de texto de la paleta
    // contra el fondo de ventana Y contra la tarjeta (los textos viven en
    // ambos). Si un test falla, el token se ajusta y se vuelve a medir:
    // cambiar un color sin pasar por aquí queda prohibido.

    /// AA texto normal (≥ 4.5:1) para todos los colores usados como texto.
    #[test]
    fn contraste_aa_de_todo_el_texto_en_ambos_modos() {
        for (nombre, p) in [("dark", &DARK), ("light", &LIGHT)] {
            for (fondo_name, fondo) in [("bg", p.bg), ("surface", p.surface)] {
                for (color_name, fg) in [
                    ("text", p.text),
                    ("dim", p.dim),
                    ("accent_text", p.accent_text),
                    ("ok", p.ok),
                    ("warn", p.warn),
                    ("err", p.err),
                ] {
                    let ratio = contrast_ratio(fg, fondo);
                    assert!(
                        ratio >= 4.5,
                        "{nombre}/{fondo_name}: {color_name} mide {ratio:.2}:1 (< 4.5 AA)"
                    );
                }
            }
        }
    }

    /// AA texto grande y componentes gráficos (≥ 3:1): el violeta de marca
    /// como texto GRANDE (hero/spinner, ≥ 24 px) y la tinta de los botones
    /// rellenos de acento (que sí es texto normal → exije 4.5).
    #[test]
    fn contraste_de_acento_grande_y_botones_rellenos() {
        for (nombre, p) in [("dark", &DARK), ("light", &LIGHT)] {
            for (fondo_name, fondo) in [("bg", p.bg), ("surface", p.surface)] {
                let ratio = contrast_ratio(p.accent, fondo);
                assert!(
                    ratio >= 3.0,
                    "{nombre}/{fondo_name}: accent grande mide {ratio:.2}:1 (< 3)"
                );
            }
            // Texto de botón primario: tinta fija sobre relleno de acento
            // (y su hover, que también es un fondo posible).
            let sobre_acento = contrast_ratio(INK_ON_ACCENT, p.accent);
            let sobre_hover = contrast_ratio(INK_ON_ACCENT, p.accent_hover);
            assert!(
                sobre_acento >= 4.5 && sobre_hover >= 4.5,
                "{nombre}: tinta sobre acento {sobre_acento:.2}:1 / hover {sobre_hover:.2}:1"
            );
        }
    }

    /// Sanity del cálculo contra los valores publicados de WCAG: negro sobre
    /// blanco = 21:1 exacto; #767676 sobre blanco = 4.54:1 (el gris mínimo
    /// AA citado por el W3C).
    #[test]
    fn ratio_de_contraste_cae_con_los_valores_conocidos() {
        assert!((contrast_ratio(Color::BLACK, Color::WHITE) - 21.0).abs() < 0.01);
        let gris = rgb!(0x76, 0x76, 0x76);
        let ratio = contrast_ratio(gris, Color::WHITE);
        assert!(
            (ratio - 4.54).abs() < 0.05,
            "#767676 sobre blanco debe medir ~4.54:1, mide {ratio:.3}"
        );
    }

    /// La tinta de los botones y el acento-texto son tokens independientes
    /// del modo: un cambio de tema no puede romper el contraste por descuido.
    #[test]
    fn tokens_de_acceso_no_cambian_con_el_modo() {
        assert_eq!(DARK.accent_text, ACCENT_TEXT);
        assert_eq!(LIGHT.accent_text, ACCENT_TEXT_LIGHT);
        assert_eq!(INK_ON_ACCENT, rgb!(0x17, 0x18, 0x1C));
    }
}
