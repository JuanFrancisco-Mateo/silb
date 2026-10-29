//! Colores, espacios, esquinas y tamaños de letra de Silb (diseño 2026).
//!
//! Todo el aspecto visual sale de aquí: nada de colores ni medidas sueltas
//! en la interfaz (`app.rs`). Ideas: diseño tranquilo, oscuro por defecto, un
//! solo acento violeta, sin desenfoques, sin animaciones de adorno y rejilla
//! de 8 px.
//!
//! Personalización (R7 — Personalización II): en Ajustes → Apariencia el
//! usuario puede cambiar CADA color de la paleta con un hex (`#RRGGBB` o
//! `#RGB`). Los cambios se guardan en `Config::palette`; `resolve_palette`
//! junta los colores de serie con los del usuario (un hex mal escrito usa el
//! color de serie, nunca rompe). El resultado se deja en un hueco del hilo
//! (`set_palette_override`) para que los estilos de los controles lean la
//! paleta activa sin tocar las 50 llamadas que los usan; solo está puesto
//! mientras se dibuja y se quita al salir (fuera de ahí valen los colores de
//! serie oscuro/claro).

use std::cell::RefCell;
use std::sync::Arc;

use iced::border::Radius;
use iced::gradient::{Gradient, Linear};
use iced::theme::Palette;
use iced::widget::{
    button, checkbox as check, container, pick_list as pick, progress_bar, rule,
    scrollable as scroll, text_editor as editor, text_input as input, toggler as toggle,
};
use iced::{Background, Border, Color, Padding, Radians, Shadow, Theme};

use crate::config::Config;

/// Color desde hex `0xRRGGBB` (fijo, calculado al compilar).
macro_rules! rgb {
    ($r:expr, $g:expr, $b:expr) => {
        Color::from_rgb($r as f32 / 255.0, $g as f32 / 255.0, $b as f32 / 255.0)
    };
}

// ─────────────────────────────── Tokens: color ───────────────────────────────
// Paleta grafito (oscura, por defecto). El acento violeta #7C6CFF es el mismo
// en los dos modos; ok/aviso/error se oscurecen un poco en claro para llegar
// al contraste AA.
//
// Los valores de la paleta OSCURA son los medidos en Figma
// (`design/openwhispr-figma/`): tarjeta #1D1E24, campo #191A20,
// texto #F1F0F5, secundario #A9A8B5, borde #303139 y acento activo #8D7AFF.
pub const BG: Color = rgb!(0x17, 0x18, 0x1C); // grafito fondo
pub const SURFACE: Color = rgb!(0x1D, 0x1E, 0x24); // tarjetas (diseño)
pub const SURFACE_ALT: Color = rgb!(0x19, 0x1A, 0x20); // campos y selects (diseño)
pub const TEXT: Color = rgb!(0xF1, 0xF0, 0xF5);
pub const DIM: Color = rgb!(0xA9, 0xA8, 0xB5);
pub const ACCENT: Color = rgb!(0x7C, 0x6C, 0xFF); // violeta único
pub const ACCENT_HOVER: Color = rgb!(0x8D, 0x7A, 0xFF);
// Acento para TEXTO normal (el violeta de marca no llega): #7C6CFF sobre la
// tarjeta grafito se queda en ~4.2:1 (por debajo del 4.5:1 que pide WCAG para
// texto pequeño). #8F82FF (y #6652F2 en claro) llegan a 4.5:1 o más sobre el
// fondo y sobre la tarjeta en los dos modos (hay tests). El violeta de marca
// se queda para rellenos, piezas grandes (≥ 3:1) y estados.
pub const ACCENT_TEXT: Color = rgb!(0x8F, 0x82, 0xFF);
pub const OK: Color = rgb!(0x3F, 0xB9, 0x5F);
pub const WARN: Color = rgb!(0xF5, 0xA6, 0x23);
// Rojo medido para AA sobre la tarjeta oscura: #E54B4F se quedaba en ~4.2:1;
// este pasa de 4.5:1 sobre el fondo y la tarjeta (ver el test de contraste).
pub const ERR: Color = rgb!(0xEB, 0x57, 0x57);
pub const BORDER: Color = rgb!(0x30, 0x31, 0x39);

pub const BG_LIGHT: Color = rgb!(0xF6, 0xF5, 0xF2);
pub const SURFACE_LIGHT: Color = rgb!(0xFF, 0xFF, 0xFF);
pub const SURFACE_ALT_LIGHT: Color = rgb!(0xEB, 0xEB, 0xEF);
pub const TEXT_LIGHT: Color = rgb!(0x17, 0x18, 0x1C);
pub const DIM_LIGHT: Color = rgb!(0x6B, 0x6B, 0x76);
// Hover del acento en claro: más CLARO que el acento (en oscuro el hover sube
// de luz). Motivo medido: la tinta de los botones (#17181C) sobre un hover
// oscuro (#6652F2) daba 3.42:1 (por debajo de AA); sobre #9A8CFF da 6.4:1. Y
// el hover más claro es lo normal en modo claro.
pub const ACCENT_HOVER_LIGHT: Color = rgb!(0x9A, 0x8C, 0xFF);
// Acento para texto en claro (hace lo mismo que ACCENT_TEXT en oscuro): llega
// a 4.5:1 sobre el fondo claro y sobre la tarjeta blanca. NO es el hover: son
// dos cosas distintas (texto sobre fondo frente a tinta sobre relleno) y cada
// una necesita su tono.
pub const ACCENT_TEXT_LIGHT: Color = rgb!(0x66, 0x52, 0xF2);
// Verde y ámbar oscurecidos para AA en claro: los de antes (#2F9E53,
// #D98E0B) se quedaban en ~3.1:1 y ~2.7:1 sobre blanco (texto pequeño < 4.5).
pub const OK_LIGHT: Color = rgb!(0x1F, 0x7A, 0x3E);
pub const WARN_LIGHT: Color = rgb!(0x8F, 0x63, 0x00);
pub const ERR_LIGHT: Color = rgb!(0xC9, 0x3E, 0x3E);
pub const BORDER_LIGHT: Color = rgb!(0xD9, 0xD9, 0xE0);

/// Tinta para texto sobre un botón LLENO de acento (por ejemplo «Aplicar»):
/// un casi negro que llega a 4.5:1 sobre #7C6CFF en los dos modos (el fondo
/// del modo no vale aquí: en claro daría ~3.5:1).
pub const INK_ON_ACCENT: Color = rgb!(0x17, 0x18, 0x1C);

// ─────────── Marco del rediseño (pantalla Dictado de Figma) ───────────────────
// Colores que NO se pueden cambiar en Ajustes → Apariencia: son la «piel» del
// marco del diseño. Se sacan por modo desde [`chrome`], que además toma de la
// paleta activa el texto, el secundario y el acento (así lo que cambie el
// usuario manda también en la marca y en los textos).
pub const SIDEBAR: Color = rgb!(0x13, 0x14, 0x18); // barra lateral
pub const WINDOW_BAR: Color = rgb!(0x15, 0x16, 0x1A); // barra de ventana propia
pub const KEYCAP: Color = rgb!(0x24, 0x25, 0x2D); // keycaps de atajo, badges
pub const NAV_ACTIVE: Color = rgb!(0x28, 0x23, 0x38); // nav activo (violeta tenue)
pub const DIVIDER: Color = rgb!(0x27, 0x28, 0x2F); // separadores internos
                                                    // Texto tenue (rótulos, versión, pistas): 11 px del diseño. MEDIDO: 4.54:1
                                                    // sobre el fondo de la ventana y la barra lateral, pero 4.25:1 sobre la
                                                    // tarjeta y 4.44:1 sobre el campo — por debajo de AA 4.5:1 para texto
                                                    // pequeño (el test de contraste lo deja anotado: el valor viene del
                                                    // diseño y no se cambia por cuenta propia).
pub const FAINT: Color = rgb!(0x7F, 0x80, 0x8D);

// Los mismos papeles en modo claro (Figma solo trae el oscuro: estos tonos son
// la traducción a claro, elegidos para mantener el orden fondo < barra <
// tarjeta y el contraste del texto tenue).
pub const SIDEBAR_LIGHT: Color = rgb!(0xF0, 0xF0, 0xF4);
pub const WINDOW_BAR_LIGHT: Color = rgb!(0xF3, 0xF3, 0xF7);
pub const KEYCAP_LIGHT: Color = rgb!(0xE8, 0xE8, 0xEE);
pub const NAV_ACTIVE_LIGHT: Color = rgb!(0xE7, 0xE3, 0xFF);
pub const DIVIDER_LIGHT: Color = rgb!(0xE3, 0xE3, 0xEA);
pub const FAINT_LIGHT: Color = rgb!(0x5F, 0x5F, 0x6A);

// ─────────────────────── Contraste WCAG (medido, no estimado) ────────────────
// Fórmulas de WCAG 2.1: luminancia relativa y ratio (1..21). Son públicas
// porque los tests de accesibilidad (R4) miden con ellas los pares REALES de
// colores que usa la app; cambiar un color sin volver a medir no pasa los
// tests.

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

/// Ratio de contraste WCAG (1..21). AA para texto normal ≥ 4.5; texto grande y
/// piezas gráficas ≥ 3.
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
    /// Acento para TEXTO normal (AA ≥ 4.5:1 sobre fondo y tarjeta).
    /// `accent` se usa para rellenos y piezas grandes (≥ 3:1).
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

// ─────────────────────────── Personalización (R7-II) ──────────────────────────
//
// `Config::palette` guarda `Option<PaletteConfig>`: cada campo es un hex
// opcional (`"#RRGGBB"` o `"#RGB"`, con `#` o sin él, da igual mayúsculas).
// Cada cambio se aplica sobre la paleta base del modo (oscuro o claro); un
// campo vacío o mal escrito usa el de serie de ese modo (la app no rompe ni
// da pánico por un hex basura). Se cambia campo a campo, no la paleta
// entera: así se puede tocar solo el acento sin perder el resto de colores.

/// Cambios de color campo a campo. Cada `Option<String>` es un hex bueno o
/// `None` (= el color de serie del modo activo). Los `None` no se escriben en
/// el JSON, para que el config quede limpio.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PaletteConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface_alt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dim: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent_hover: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warn: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<String>,
}

impl PaletteConfig {
    /// ¿Hay AL MENOS un color cambiado? `false` = paleta entera de serie.
    pub fn is_empty(&self) -> bool {
        self.bg.is_none()
            && self.surface.is_none()
            && self.surface_alt.is_none()
            && self.text.is_none()
            && self.dim.is_none()
            && self.accent.is_none()
            && self.accent_hover.is_none()
            && self.accent_text.is_none()
            && self.ok.is_none()
            && self.warn.is_none()
            && self.err.is_none()
            && self.border.is_none()
    }

    /// Borra TODOS los cambios → vuelve a la paleta de serie del modo activo.
    /// La interfaz lo usa en el botón «Restaurar todo».
    pub fn clear(&mut self) {
        self.bg = None;
        self.surface = None;
        self.surface_alt = None;
        self.text = None;
        self.dim = None;
        self.accent = None;
        self.accent_hover = None;
        self.accent_text = None;
        self.ok = None;
        self.warn = None;
        self.err = None;
        self.border = None;
    }

    /// Devuelve un campo concreto a su color de serie (`None`). La interfaz lo
    /// usa en el botón «Restaurar» de cada campo.
    pub fn clear_field(&mut self, field: PaletteField) {
        let slot: &mut Option<String> = match field {
            PaletteField::Bg => &mut self.bg,
            PaletteField::Surface => &mut self.surface,
            PaletteField::SurfaceAlt => &mut self.surface_alt,
            PaletteField::Text => &mut self.text,
            PaletteField::Dim => &mut self.dim,
            PaletteField::Accent => &mut self.accent,
            PaletteField::AccentHover => &mut self.accent_hover,
            PaletteField::AccentText => &mut self.accent_text,
            PaletteField::Ok => &mut self.ok,
            PaletteField::Warn => &mut self.warn,
            PaletteField::Err => &mut self.err,
            PaletteField::Border => &mut self.border,
        };
        *slot = None;
    }
}

/// Identifica un campo de `PaletteConfig` para que la interfaz pueda
/// «restaurar este campo» sin conocer la estructura de dentro. Lo usa
/// `Message::PaletteFieldReset`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaletteField {
    Bg,
    Surface,
    SurfaceAlt,
    Text,
    Dim,
    Accent,
    AccentHover,
    AccentText,
    Ok,
    Warn,
    Err,
    Border,
}

impl PaletteField {
    /// Todos los campos, en el orden fijo que usa la interfaz.
    pub const ALL: &'static [PaletteField] = &[
        PaletteField::Bg,
        PaletteField::Surface,
        PaletteField::SurfaceAlt,
        PaletteField::Text,
        PaletteField::Dim,
        PaletteField::Accent,
        PaletteField::AccentHover,
        PaletteField::AccentText,
        PaletteField::Ok,
        PaletteField::Warn,
        PaletteField::Err,
        PaletteField::Border,
    ];

    /// Nombre en español para la fila de la interfaz (tal cual).
    pub fn label(self) -> &'static str {
        match self {
            PaletteField::Bg => "Fondo de ventana",
            PaletteField::Surface => "Tarjetas / inputs",
            PaletteField::SurfaceAlt => "Hover / superficies 2",
            PaletteField::Text => "Texto principal",
            PaletteField::Dim => "Texto atenuado",
            PaletteField::Accent => "Acento",
            PaletteField::AccentHover => "Acento (hover)",
            PaletteField::AccentText => "Acento (texto)",
            PaletteField::Ok => "Ok",
            PaletteField::Warn => "Aviso",
            PaletteField::Err => "Error",
            PaletteField::Border => "Bordes",
        }
    }

    /// Texto de ayuda CORTO (lo técnico va en «Ajustes avanzados», pero esto se
    /// enseña igual porque cambiar colores ES la parte avanzada de Apariencia y
    /// el usuario tiene que saber qué cambia cada campo).
    pub fn help(self) -> &'static str {
        match self {
            PaletteField::Bg => "Fondo de la ventana principal.",
            PaletteField::Surface => "Color de las tarjetas y los campos de texto.",
            PaletteField::SurfaceAlt => "Fondo al pasar el cursor sobre elementos.",
            PaletteField::Text => "Color principal del texto (≥ 4.5:1 sobre fondo y tarjeta).",
            PaletteField::Dim => "Texto secundario (labels, pistas). Mide ≥ 4.5:1 también.",
            PaletteField::Accent => {
                "Color de marca (botones primarios, foco,Checks). Acepta ≥ 3:1."
            }
            PaletteField::AccentHover => "Acento al pasar el cursor sobre el botón primario.",
            PaletteField::AccentText => "Acento usado como TEXTO normal (≥ 4.5:1 sobre fondo).",
            PaletteField::Ok => "Estados correctos (verde). Mide ≥ 4.5:1 como texto.",
            PaletteField::Warn => "Estados de aviso (ámbar). Mide ≥ 4.5:1 como texto.",
            PaletteField::Err => "Estados de error (rojo). Mide ≥ 4.5:1 como texto.",
            PaletteField::Border => "Bordes sutiles de tarjetas y separadores.",
        }
    }
}

/// Lee un hex de CSS (`#RRGGBB`, `#RGB`, con `#` o sin él; da igual
/// mayúsculas) y devuelve un `Color` opaco. `None` si el texto no vale — la
/// interfaz lo usa para «campo mal escrito → se ignora y se pinta el color de
/// serie» sin romper el dibujo.
///
/// Acepta 3 o 6 dígitos hex (como CSS). 8 NO: la transparencia no se cambia
/// (la opacidad global es otro ajuste). El `#` del principio es opcional.
pub fn parse_hex(raw: &str) -> Option<Color> {
    let s = raw.trim();
    let s = s.strip_prefix('#').unwrap_or(s);
    let s = s.strip_prefix("0x").unwrap_or(s);
    let s = s.strip_prefix("0X").unwrap_or(s);
    let bytes = s.as_bytes();
    let rgb @ [r, g, b] = match bytes.len() {
        3 => {
            // #RGB → se pasa a #RRGGBB (cada dígito se repite).
            let r = hex_nibble(bytes[0])?;
            let g = hex_nibble(bytes[1])?;
            let b = hex_nibble(bytes[2])?;
            [r * 17, g * 17, b * 17]
        }
        6 => {
            let r = u8::from_str_radix(std::str::from_utf8(&bytes[0..2]).ok()?, 16).ok()?;
            let g = u8::from_str_radix(std::str::from_utf8(&bytes[2..4]).ok()?, 16).ok()?;
            let b = u8::from_str_radix(std::str::from_utf8(&bytes[4..6]).ok()?, 16).ok()?;
            [r, g, b]
        }
        _ => return None,
    };
    let _ = rgb;
    Some(Color::from_rgb(
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
    ))
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Junta la paleta que se usa (colores de serie del modo + cambios del
/// usuario). Si `cfg.palette` es `None` o todos sus campos son `None`,
/// devuelve una copia de la base oscura o clara (igual que antes de R7-II).
/// Cada hex mal escrito se ignora y se usa el color de serie del campo.
pub fn resolve_palette(cfg: &Config, dark: bool) -> UiPalette {
    let base = if dark { DARK } else { LIGHT };
    let Some(overrides) = cfg.palette.as_ref() else {
        return base;
    };
    fn pick(field: PaletteField, overrides: &PaletteConfig, default: Color) -> Color {
        let raw = match field {
            PaletteField::Bg => &overrides.bg,
            PaletteField::Surface => &overrides.surface,
            PaletteField::SurfaceAlt => &overrides.surface_alt,
            PaletteField::Text => &overrides.text,
            PaletteField::Dim => &overrides.dim,
            PaletteField::Accent => &overrides.accent,
            PaletteField::AccentHover => &overrides.accent_hover,
            PaletteField::AccentText => &overrides.accent_text,
            PaletteField::Ok => &overrides.ok,
            PaletteField::Warn => &overrides.warn,
            PaletteField::Err => &overrides.err,
            PaletteField::Border => &overrides.border,
        };
        raw.as_deref().and_then(parse_hex).unwrap_or(default)
    }
    UiPalette {
        bg: pick(PaletteField::Bg, overrides, base.bg),
        surface: pick(PaletteField::Surface, overrides, base.surface),
        surface_alt: pick(PaletteField::SurfaceAlt, overrides, base.surface_alt),
        text: pick(PaletteField::Text, overrides, base.text),
        dim: pick(PaletteField::Dim, overrides, base.dim),
        accent: pick(PaletteField::Accent, overrides, base.accent),
        accent_hover: pick(PaletteField::AccentHover, overrides, base.accent_hover),
        accent_text: pick(PaletteField::AccentText, overrides, base.accent_text),
        ok: pick(PaletteField::Ok, overrides, base.ok),
        warn: pick(PaletteField::Warn, overrides, base.warn),
        err: pick(PaletteField::Err, overrides, base.err),
        border: pick(PaletteField::Border, overrides, base.border),
    }
}

// ───────────── Presets (un clic cambia 12 campos a la vez) ───────────────
//
// Pensados para los dos modos (oscuro y claro) y 3 acentos distintos. NO se
// mezclan con la paleta base: cada preset CAMBIA todos los campos (acento
// incluido). Son para «inspirarse»: se pulsa uno y luego se afina campo a
// campo.

/// Paleta completa de un preset: cambia los 12 campos de golpe (acento
/// incluido). Sirve de punto de partida; luego se afina campo a campo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PalettePreset {
    /// Paleta oscura por defecto (violeta + grafito).
    Violeta,
    /// Grafito puro, acento cian sobrio (lectura larga, poco distractivo).
    Grafito,
    /// Paleta clara por defecto.
    Claro,
    /// Verde bosque sobre fondo crema (lectura larga en claro, contraste suave).
    Bosque,
    /// Azul océano sobre fondo tinta (modo oscuro alternativo al grafito).
    Oceánico,
}

impl PalettePreset {
    /// Todos los presets, en el orden en que salen en Ajustes.
    pub const ALL: &'static [PalettePreset] = &[
        PalettePreset::Violeta,
        PalettePreset::Grafito,
        PalettePreset::Claro,
        PalettePreset::Bosque,
        PalettePreset::Oceánico,
    ];

    /// Nombre tal cual se enseña en el selector.
    pub fn label(self) -> &'static str {
        match self {
            PalettePreset::Violeta => "Violeta (oscuro por defecto)",
            PalettePreset::Grafito => "Grafito (cian sobrio)",
            PalettePreset::Claro => "Claro (violeta)",
            PalettePreset::Bosque => "Bosque (verde sobre crema)",
            PalettePreset::Oceánico => "Oceánico (azul sobre tinta)",
        }
    }

    /// Paleta del preset para la rama oscura o clara. Devuelve la paleta
    /// entera (los 12 campos), así que sirve de «fuente de verdad» para
    /// rellenar `Config::palette`.
    pub fn colors(self, dark: bool) -> UiPalette {
        match (self, dark) {
            // Oscuros
            (PalettePreset::Violeta, true) => DARK,
            (PalettePreset::Claro, true) => DARK, // preset «claro» en modo oscuro → cae al violeta oscuro
            (PalettePreset::Grafito, true) => UiPalette {
                bg: rgb!(0x14, 0x16, 0x1A),
                surface: rgb!(0x1B, 0x1E, 0x24),
                surface_alt: rgb!(0x24, 0x27, 0x2E),
                text: rgb!(0xEC, 0xED, 0xF0),
                dim: rgb!(0x92, 0x96, 0xA0),
                accent: rgb!(0x3E, 0xC8, 0xC8), // cian sobrio
                accent_hover: rgb!(0x5C, 0xDA, 0xDA),
                accent_text: rgb!(0x5C, 0xDA, 0xDA),
                ok: rgb!(0x4E, 0xC8, 0x6E),
                warn: rgb!(0xE8, 0xA6, 0x3A),
                err: rgb!(0xE6, 0x6B, 0x6B),
                border: rgb!(0x29, 0x2C, 0x34),
            },
            (PalettePreset::Bosque, true) => UiPalette {
                bg: rgb!(0x16, 0x1F, 0x18),
                surface: rgb!(0x1E, 0x29, 0x21),
                surface_alt: rgb!(0x26, 0x32, 0x2A),
                text: rgb!(0xEC, 0xEF, 0xE8),
                dim: rgb!(0x96, 0xA2, 0x94),
                accent: rgb!(0x6E, 0xC8, 0x6B),
                accent_hover: rgb!(0x8A, 0xDC, 0x86),
                accent_text: rgb!(0x8A, 0xDC, 0x86),
                ok: rgb!(0x6E, 0xC8, 0x6B),
                warn: rgb!(0xE8, 0xB6, 0x4A),
                err: rgb!(0xE6, 0x6B, 0x6B),
                border: rgb!(0x2A, 0x36, 0x2D),
            },
            (PalettePreset::Oceánico, true) => UiPalette {
                bg: rgb!(0x10, 0x14, 0x1C),
                surface: rgb!(0x18, 0x1E, 0x28),
                surface_alt: rgb!(0x20, 0x27, 0x33),
                text: rgb!(0xEC, 0xEF, 0xF4),
                dim: rgb!(0x90, 0x9A, 0xA8),
                accent: rgb!(0x4F, 0x8E, 0xE8),
                accent_hover: rgb!(0x6E, 0xA8, 0xF2),
                accent_text: rgb!(0x6E, 0xA8, 0xF2),
                ok: rgb!(0x4E, 0xC8, 0x8E),
                warn: rgb!(0xE8, 0xB6, 0x4A),
                err: rgb!(0xE6, 0x6B, 0x6B),
                border: rgb!(0x25, 0x2D, 0x3A),
            },
            // Claros
            (PalettePreset::Violeta, false) | (PalettePreset::Claro, false) => LIGHT,
            (PalettePreset::Grafito, false) => UiPalette {
                bg: rgb!(0xF4, 0xF5, 0xF7),
                surface: rgb!(0xFF, 0xFF, 0xFF),
                surface_alt: rgb!(0xE7, 0xE9, 0xED),
                text: rgb!(0x14, 0x16, 0x1A),
                dim: rgb!(0x68, 0x6C, 0x76),
                accent: rgb!(0x1F, 0x7A, 0x82), // cian sobrio oscuro para AA en claro
                accent_hover: rgb!(0x26, 0x90, 0x9A),
                accent_text: rgb!(0x1F, 0x7A, 0x82),
                ok: rgb!(0x1F, 0x7A, 0x3E),
                warn: rgb!(0x8F, 0x63, 0x00),
                err: rgb!(0xC9, 0x3E, 0x3E),
                border: rgb!(0xD6, 0xD8, 0xDE),
            },
            (PalettePreset::Bosque, false) => UiPalette {
                bg: rgb!(0xF6, 0xF4, 0xEA),
                surface: rgb!(0xFF, 0xFC, 0xF1),
                surface_alt: rgb!(0xE8, 0xE4, 0xD2),
                text: rgb!(0x1A, 0x26, 0x18),
                dim: rgb!(0x66, 0x72, 0x62),
                accent: rgb!(0x2E, 0x6E, 0x3A),
                accent_hover: rgb!(0x3A, 0x82, 0x48),
                accent_text: rgb!(0x2E, 0x6E, 0x3A),
                ok: rgb!(0x2E, 0x6E, 0x3A),
                warn: rgb!(0x8F, 0x63, 0x00),
                err: rgb!(0xC9, 0x3E, 0x3E),
                border: rgb!(0xD8, 0xD2, 0xBE),
            },
            (PalettePreset::Oceánico, false) => UiPalette {
                bg: rgb!(0xF4, 0xF6, 0xFA),
                surface: rgb!(0xFF, 0xFF, 0xFF),
                surface_alt: rgb!(0xE3, 0xE8, 0xF0),
                text: rgb!(0x12, 0x18, 0x24),
                dim: rgb!(0x66, 0x70, 0x82),
                accent: rgb!(0x25, 0x55, 0xA0),
                accent_hover: rgb!(0x33, 0x6A, 0xC0),
                accent_text: rgb!(0x25, 0x55, 0xA0),
                ok: rgb!(0x1F, 0x7A, 0x3E),
                warn: rgb!(0x8F, 0x63, 0x00),
                err: rgb!(0xC9, 0x3E, 0x3E),
                border: rgb!(0xD2, 0xD8, 0xE2),
            },
        }
    }
}

/// Pasa una `UiPalette` ya resuelta al `PaletteConfig` equivalente (cada
/// campo como `"#RRGGBB"`). Lo usa el botón «Aplicar preset» y la
/// importación de JSON al config.
pub fn palette_to_config(p: UiPalette) -> PaletteConfig {
    fn hx(c: Color) -> String {
        let r = (c.r * 255.0).round().clamp(0.0, 255.0) as u8;
        let g = (c.g * 255.0).round().clamp(0.0, 255.0) as u8;
        let b = (c.b * 255.0).round().clamp(0.0, 255.0) as u8;
        format!("#{r:02X}{g:02X}{b:02X}")
    }
    PaletteConfig {
        bg: Some(hx(p.bg)),
        surface: Some(hx(p.surface)),
        surface_alt: Some(hx(p.surface_alt)),
        text: Some(hx(p.text)),
        dim: Some(hx(p.dim)),
        accent: Some(hx(p.accent)),
        accent_hover: Some(hx(p.accent_hover)),
        accent_text: Some(hx(p.accent_text)),
        ok: Some(hx(p.ok)),
        warn: Some(hx(p.warn)),
        err: Some(hx(p.err)),
        border: Some(hx(p.border)),
    }
}

// ───────────── Hilo actual: deja la paleta resuelta para los estilos ─────
//
// Los controles piden la `UiPalette` dentro de sus funciones de estilo (que
// solo reciben `&Theme` por la API de iced). Para que lean la paleta del
// usuario sin tocar las 50 llamadas, se guarda la paleta activa en el hilo
// mientras se dibuja. Fuera del dibujo, `tokens(theme)` devuelve los colores
// de serie oscuro/claro (los tests de contraste cuentan con eso).

thread_local! {
    static OVERRIDE: RefCell<Option<Arc<UiPalette>>> = const { RefCell::new(None) };
}

/// Deja la paleta que verán los estilos hasta el próximo
/// `clear_palette_override`. Se pone alrededor de `WhisprApp::view()` y
/// `bubble_view()` (el mismo hilo de la interfaz). `None` = sin cambio.
pub fn set_palette_override(p: Option<Arc<UiPalette>>) {
    OVERRIDE.with(|c| *c.borrow_mut() = p);
}

/// Quita el cambio. Lo llama el destructor de `PaletteOverrideScope`.
pub fn clear_palette_override() {
    OVERRIDE.with(|c| *c.borrow_mut() = None);
}

/// Ayudante que se pone al crearse y se quita al soltarse. Así un `?` o un
/// pánico al dibujar no deja una paleta fantasma para la siguiente vuelta.
pub struct PaletteOverrideScope {
    previous: Option<Arc<UiPalette>>,
}

impl PaletteOverrideScope {
    /// Pone `p` como paleta activa y se guarda la anterior para devolverla al
    /// soltarse (en `Drop`).
    pub fn new(p: UiPalette) -> Self {
        let arc = Arc::new(p);
        let previous = OVERRIDE.with(|c| {
            let prev = c.borrow_mut().replace(arc);
            // `prev` es `Option<Arc<UiPalette>>`; `replace` devuelve el valor
            // de antes sin envolverlo dos veces.
            prev
        });
        Self { previous }
    }
}

impl Drop for PaletteOverrideScope {
    fn drop(&mut self) {
        let prev = self.previous.take();
        OVERRIDE.with(|c| *c.borrow_mut() = prev);
    }
}

fn override_active() -> Option<Arc<UiPalette>> {
    OVERRIDE.with(|c| c.borrow().clone())
}

// ─────────────────────────────── Medidas: geometría ───────────────────────────────
// Rejilla de 8 px.
pub const SPACE_1: u16 = 4; // medio paso
pub const SPACE_2: u16 = 8;
pub const SPACE_3: u16 = 16;
pub const SPACE_4: u16 = 24;
pub const SPACE_5: u16 = 32;
pub const SPACE_6: u16 = 48;

// Esquinas de 8–10 px (en el marco: 12 px en tarjetas, 5 en teclas).
pub const RADIUS_SM: f32 = 8.0;
pub const RADIUS_MD: f32 = 10.0;
/// Esquina de las tarjetas del rediseño (pantalla Dictado: 12 px).
pub const RADIUS_LG: f32 = 12.0;
/// Esquina de las teclas de atajo y las etiquetas del rediseño (5 px).
pub const RADIUS_KEYCAP: f32 = 5.0;
/// Radio del botón circular de grabación (círculo de 80 px = radio 40).
pub const RADIUS_CIRCLE: f32 = 40.0;

/// Relleno de los botones de acción del marco (36 px de alto con un icono de
/// 16 px): el del diseño, «Action» y «Copiar texto» de la pantalla Dictado.
pub const ACTION_BUTTON_PADDING: [f32; 2] = [10.0, 12.0];
/// Relleno de una tecla de atajo (23 px de alto con texto de 11 px).
pub const KEYCAP_PADDING: [f32; 2] = [4.0, 7.0];
/// Alto de la barra de ventana propia (`Window bar` del diseño).
pub const WINDOW_BAR_HEIGHT: f32 = 40.0;

// ───────────── Rejilla única de las 5 pestañas de Ajustes (pantalla 3:604) ───
//
// La coherencia no se copia de una pantalla: sale de esta rejilla, la única
// fuente de medidas de las cinco pestañas (General, Apariencia, Dictado,
// Corrección IA, Avanzado). Cero medidas sueltas en la interfaz.
// Todas salen del diseño de la pantalla Ajustes
// (medidas del diseño de Figma):
//
//   nodo            medida
//   3:638 / 3:639   contenido 916 px (márgenes de página de 40, en shell.rs)
//   3:656 (Fila)    68 px de alto · rótulo en x=20 → margen lateral 20
//   3:657 (Rótulo)  columna de rótulo/ayuda 606 px
//   3:660 / 3:667   250 px de ancho, x=646 → borde derecho x=896 (= margen 20)
//   3:692 (Switch)  36×22, x=860 → el MISMO borde derecho x=896
//   3:691 (Estado)  etiqueta de estado con borde derecho a 20 px del switch
//   3:672           fila del modelo: 82 px (icono de 42 + 20 arriba y abajo)
//   3:654           título de sección de 15 px, tarjeta a 10 px (y=25)
//   3:653→3:670     separación entre secciones 20 px (161 → 181)
//   3:642 / 3:643   pestaña de 43 px de alto, separación 28 px (x=0 → 113)
pub const GRID_CONTENT_WIDTH: f32 = 916.0;
pub const GRID_LABEL_WIDTH: f32 = 606.0;
pub const GRID_CONTROL_WIDTH: f32 = 250.0;
pub const GRID_ROW_HEIGHT: f32 = 68.0;
pub const GRID_ROW_PAD_V: f32 = 15.0;
pub const GRID_ROW_PAD_H: f32 = 20.0;
pub const GRID_MODEL_ROW_HEIGHT: f32 = 82.0;
pub const GRID_SECTION_GAP: f32 = 20.0;
pub const GRID_SECTION_TITLE_GAP: f32 = 10.0;
pub const GRID_TAB_HEIGHT: f32 = 43.0;
pub const GRID_TAB_GAP: f32 = 28.0;
pub const GRID_TOGGLE_SIZE: f32 = 22.0;
/// Ancho del interruptor del diseño (nodo `3:692`). NO se aplica al control:
/// iced pinta el suyo al doble del alto (`size` × 2 = 44 px) y no hace caso al
/// ancho pedido, así que la app lleva el interruptor 8 px más ancho que el
/// diseño. Lo que la rejilla fija es su BORDE DERECHO, que sí coincide.
pub const GRID_TOGGLE_WIDTH: f32 = 36.0;
pub const GRID_TOGGLE_GAP: f32 = 20.0;
/// Separación entre bloques de la página (cabecera → pestañas → contenido),
/// nodo `3:639`: 22 px (92 → 114 y 157 → 179).
pub const GRID_BLOCK_GAP: f32 = 22.0;

/// Ancho de la barra lateral (`Sidebar` del diseño).
pub const SIDEBAR_WIDTH: f32 = 204.0;
/// Diámetro del botón circular de grabación (`Record button` del diseño).
pub const RECORD_BUTTON_SIZE: f32 = 80.0;

// Tamaños de fuente.
pub const FONT_XS: u16 = 11;
pub const FONT_SM: u16 = 12;
pub const FONT_MD: u16 = 14;
pub const FONT_LG: u16 = 16;
pub const FONT_XL: u16 = 20;
pub const FONT_HERO: u16 = 24; // label del estado del micrófono
                               // (FONT_DOT se eliminó: el punto de estado del hero ahora usa FONT_XL)

/// Tamaños de letra según lo que se elija en Ajustes (`Config::font_size()`).
/// Factor 1.0 = los `FONT_*` de base.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontSizes {
    pub xxs: u16,
    pub sm: u16,
    pub md: u16,
    pub lg: u16,
    pub xl: u16,
    pub hero: u16,
}

/// Multiplica los tamaños base (`FONT_XS..FONT_HERO`) por un factor. Se limita
/// a 8..=64 px: un factor raro (por ejemplo 1.3 con hero=24 → 31) nunca da
/// tamaños absurdos. Se redondea al entero más cercano (la interfaz pide u16).
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

/// Tamaños de letra MEDIDOS en la pantalla Dictado de Figma. Van aparte de
/// [`FontSizes`] (la de Ajustes, que viene de la interfaz anterior): cada uno
/// sale del diseño, no de la rejilla de 8 px. Se multiplican igual que
/// `FontSizes` con el factor de Ajustes → Apariencia.
///
/// Los nombres son por PAPEL (no por px) porque son los del diseño: `chrome`
/// es el texto de 11 px (título de ventana, atajos, versión, pistas), `body`
/// el de 12 px (estado, privacidad), `control` el de 13 px (desplegables,
/// subtítulo)…
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DesignType {
    /// 9 px — sub-rótulo de marca (hoy no se usa: la marca es solo «Silb»).
    pub brand_sub: u16,
    /// 10 px — etiquetas pequeñas.
    pub badge: u16,
    /// 11 px — barra de ventana, teclas de atajo, versión, pistas, pie.
    pub chrome: u16,
    /// 12 px — estado de grabación, privacidad, rótulos de tarjeta.
    pub body: u16,
    /// 13 px — valores de los desplegables, subtítulo de página, botones.
    pub control: u16,
    /// 14 px — navegación y títulos de tarjeta.
    pub nav: u16,
    /// 18 px — el «Silb» de la marca.
    pub brand: u16,
    /// 19 px — texto transcrito (el cuerpo editable).
    pub transcript: u16,
    /// 22 px — acción principal de la tarjeta de grabación.
    pub hero: u16,
    /// 28 px — título de página.
    pub title: u16,
}

/// Aplica el factor de tamaño (el mismo límite 8..=64 px que [`font_sizes`])
/// a los tamaños del diseño.
pub fn design_type(factor: f32) -> DesignType {
    let scale = |base: u16| ((base as f32 * factor).round() as u16).clamp(8, 64);
    DesignType {
        brand_sub: scale(9),
        badge: scale(10),
        chrome: scale(11),
        body: scale(12),
        control: scale(13),
        nav: scale(14),
        brand: scale(18),
        transcript: scale(19),
        hero: scale(22),
        title: scale(28),
    }
}

/// Auto-tema: ¿el sistema está en oscuro? Lee `AppsUseLightTheme`
/// (REG_DWORD) de
/// `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize`,
/// que Windows actualiza al cambiar de tema. `true` = oscuro. Si no se puede
/// leer, `true` (oscuro, lo de serie): la configuración nunca rompe el arranque.
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

/// Linux/UNIX: mira el tema con `gsettings get
/// org.gnome.desktop.interface color-scheme` (lo que usan Hyprland/Omarchy).
/// `prefer-dark` = oscuro.
///
/// Por qué `gsettings` y no el portal de D-Bus: es el mismo dato (`color-scheme`
/// es la clave que expone el portal `org.freedesktop.appearance`) sin tener que
/// hablar D-Bus a mano. Sin `gsettings` en el PATH (equipos raros, servidor sin
/// GNOME) o si la clave no existe → `true` (oscuro, lo de serie), sin romper
/// nada, como el resto de la config. «A validar» en el binario: qué pasa en
/// sistemas sin settings-gtk en marcha.
#[cfg(not(target_os = "windows"))]
pub fn system_dark_mode() -> bool {
    use std::process::Command;

    let output = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            // La respuesta viene con apóstrofes: `'prefer-dark'`, `'default'`, `'prefer-light'`.
            let raw = String::from_utf8_lossy(&out.stdout);
            raw.contains("prefer-dark")
        }
        Ok(_) => {
            // La clave existe pero gsettings falló: sesión sin settings-gtk
            // (Hyprland pelado sin el portal) o esquema viejo.
            crate::log_debug!("theme", "gsettings sin respuesta válida; asumo oscuro");
            true
        }
        Err(err) => {
            // Sin el programa gsettings: entorno mínimo, se va a lo de serie.
            crate::log_debug!("theme", "gsettings no disponible ({err}); asumo oscuro");
            true
        }
    }
}


// ─────────────────────────────── Temas iced ───────────────────────────────

/// Tema con la paleta del modo activo (oscuro/claro de serie si el usuario no
/// ha cambiado nada). Para tests y para arrancar sin `Config`.
pub fn custom(dark: bool) -> Theme {
    custom_with_palette(if dark { &DARK } else { &LIGHT })
}

/// Tema hecho desde una `UiPalette` ya resuelta. Quien llama (WhisprApp) pasa
/// `resolve_palette(cfg, dark)` para que el `Theme` de iced tenga los cambios
/// del usuario (los 5 colores del `Palette` de iced salen de la paleta
/// resuelta, así el fondo de la ventana y los controles sin estilo propio
/// también respetan la personalización).
pub fn custom_with_palette(p: &UiPalette) -> Theme {
    Theme::custom(
        // El nombre no se usa para nada; se deja uno fijo por modo.
        format!(
            "Silb — {}",
            if is_palette_dark(p) {
                "oscuro"
            } else {
                "claro"
            }
        ),
        Palette {
            background: p.bg,
            text: p.text,
            primary: p.accent,
            success: p.ok,
            danger: p.err,
        },
    )
}

/// ¿Esta paleta «es oscura»? Se mira la luminancia relativa del fondo (WCAG):
/// por debajo del 50 %. Es la misma fórmula que usa `contrast_ratio` por
/// dentro, así que coincide con lo que ve el ojo.
fn is_palette_dark(p: &UiPalette) -> bool {
    let lum = |c: Color| -> f64 {
        let channel = |v: f32| {
            let v = v.clamp(0.0, 1.0) as f64;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b)
    };
    lum(p.bg) < 0.5
}

/// `true` si el tema activo es el oscuro. Con cambios del usuario se decide
/// por la luminancia de la paleta resuelta (antes se comparaba `bg == arc.bg`,
/// que daba siempre `true` con cambios y rompía el modo claro con paleta
/// propia). Sin cambios, por la luminancia del fondo del `Theme`.
pub fn is_dark(theme: &Theme) -> bool {
    if let Some(arc) = override_active() {
        return is_palette_dark(&arc);
    }
    is_palette_dark(&UiPalette {
        bg: theme.palette().background,
        ..DARK
    })
}

/// Paleta del tema activo. Si hay cambios puestos (con
/// `set_palette_override` / `PaletteOverrideScope`, normalmente al dibujar),
/// devuelve una referencia al `Arc` resuelto. Si no, devuelve `DARK` o
/// `LIGHT` (igual que antes de R7-II).
///
/// Las funciones de estilo de iced se ejecutan en el hilo de la interfaz al
/// dibujar, y ahí los cambios siempre están puestos, así que en la práctica
/// SIEMPRE se devuelve la paleta resuelta.
pub fn tokens(theme: &Theme) -> std::borrow::Cow<'static, UiPalette> {
    if let Some(arc) = override_active() {
        return std::borrow::Cow::Owned((*arc).clone());
    }
    let base: &'static UiPalette = if is_dark(theme) { &DARK } else { &LIGHT };
    std::borrow::Cow::Borrowed(base)
}

/// Colores y bordes del marco del rediseño (pantalla Dictado) que NO se pueden
/// cambiar en Ajustes. Se calculan una vez por vuelta con [`chrome`]: así la
/// interfaz no escribe ni un color suelto.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Chrome {
    /// Fondo de la barra de ventana propia.
    pub window_bar: Color,
    /// Fondo de la barra lateral.
    pub sidebar: Color,
    /// Fondo de tarjeta.
    pub card: Color,
    /// Fondo de campos y selects.
    pub field: Color,
    /// Fondo de keycaps de atajo y badges.
    pub keycap: Color,
    /// Fondo del elemento de navegación activo.
    pub nav_active: Color,
    /// Separadores internos (divider, bordes de cabecera/pie de tarjeta).
    pub divider: Color,
    /// Borde de tarjetas y campos.
    pub border: Color,
    /// Texto primario.
    pub text: Color,
    /// Texto secundario.
    pub secondary: Color,
    /// Texto tenue (medido por debajo de AA sobre la tarjeta: ver [`FAINT`]).
    pub faint: Color,
    /// Acento de marca.
    pub accent: Color,
    /// Acento activo (cursor, estados activos, hover del botón primario).
    pub accent_active: Color,
}

/// Calcula los colores del marco para el tema activo. De la paleta resuelta
/// (con los cambios del usuario) toma el texto, el secundario, los bordes y el
/// acento; el resto son los tonos medidos del diseño.
pub fn chrome(theme: &Theme) -> Chrome {
    let t = tokens(theme);
    let dark = is_dark(theme);
    Chrome {
        window_bar: if dark { WINDOW_BAR } else { WINDOW_BAR_LIGHT },
        sidebar: if dark { SIDEBAR } else { SIDEBAR_LIGHT },
        card: t.surface,
        field: t.surface_alt,
        keycap: if dark { KEYCAP } else { KEYCAP_LIGHT },
        nav_active: if dark { NAV_ACTIVE } else { NAV_ACTIVE_LIGHT },
        divider: if dark { DIVIDER } else { DIVIDER_LIGHT },
        border: t.border,
        text: t.text,
        secondary: t.dim,
        faint: if dark { FAINT } else { FAINT_LIGHT },
        accent: t.accent,
        accent_active: t.accent_hover,
    }
}

fn border(color: Color, width: f32, radius: f32) -> Border {
    Border {
        color,
        width,
        radius: Radius::from(radius),
    }
}

/// Transparencia absoluta (la API de iced 0.13 solo deja multiplicar). Es
/// pública: la burbuja la usa para la opacidad del ajuste (20–100 %).
pub fn with_alpha(c: Color, a: f32) -> Color {
    Color {
        r: c.r,
        g: c.g,
        b: c.b,
        a,
    }
}

// ─────────────────────────────── Estilos de widgets ───────────────────────────────

// ─────────────────────────── Estilos del marco (diseño) ───────────────────────────

/// Barra de ventana propia (`Window bar`): fondo propio y sin borde (la línea
/// de abajo la pinta la interfaz con [`divider_rule`]).
pub fn window_bar(theme: &Theme) -> container::Style {
    let c = chrome(theme);
    container::Style {
        text_color: Some(c.faint),
        background: Some(Background::Color(c.window_bar)),
        ..container::Style::default()
    }
}

/// Barra lateral (`Sidebar`): fondo propio; la línea de la derecha la pinta la
/// interfaz con un `rule` vertical.
pub fn sidebar(theme: &Theme) -> container::Style {
    let c = chrome(theme);
    container::Style {
        text_color: Some(c.text),
        background: Some(Background::Color(c.sidebar)),
        ..container::Style::default()
    }
}

/// Línea separadora del marco (1 px, color `Divider` del diseño).
pub fn divider_rule(theme: &Theme) -> rule::Style {
    rule::Style {
        color: chrome(theme).divider,
        width: 1,
        radius: Radius::from(0.0),
        fill_mode: rule::FillMode::Full,
    }
}

/// Tecla de atajo (fondo de tecla + borde de campo + esquina de 5).
pub fn keycap(theme: &Theme) -> container::Style {
    let c = chrome(theme);
    container::Style {
        text_color: Some(c.secondary),
        background: Some(Background::Color(c.keycap)),
        border: border(c.border, 1.0, RADIUS_KEYCAP),
        shadow: Shadow::default(),
    }
}

/// Botón de icono del marco (botones de ventana, ayuda, atajos a Ajustes):
/// transparente; al pasar por encima, fondo de tecla.
pub fn icon_button(theme: &Theme, status: button::Status) -> button::Style {
    let c = chrome(theme);
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => Some(Background::Color(c.keycap)),
        _ => None,
    };
    button::Style {
        background: bg,
        text_color: c.faint,
        border: border(Color::TRANSPARENT, 0.0, RADIUS_KEYCAP),
        shadow: Shadow::default(),
    }
}

/// Elemento de la barra lateral: el activo lleva fondo `nav_active` y texto
/// principal; el resto, texto secundario.
pub fn nav_item(theme: &Theme, status: button::Status, active: bool) -> button::Style {
    let c = chrome(theme);
    let bg = if active {
        Some(Background::Color(c.nav_active))
    } else {
        match status {
            button::Status::Hovered | button::Status::Pressed => Some(Background::Color(c.keycap)),
            _ => None,
        }
    };
    button::Style {
        background: bg,
        text_color: if active { c.text } else { c.secondary },
        border: border(Color::TRANSPARENT, 0.0, RADIUS_SM),
        shadow: Shadow::default(),
    }
}

/// Botón redondo de grabación (80 px de acento): al pasar por encima sube al
/// acento activo del diseño.
pub fn record(theme: &Theme, status: button::Status) -> button::Style {
    let c = chrome(theme);
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => c.accent_active,
        _ => c.accent,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: INK_ON_ACCENT,
        border: border(Color::TRANSPARENT, 0.0, RADIUS_CIRCLE),
        shadow: Shadow::default(),
    }
}

/// Interruptor del marco (`Corrección IA`): apagado con fondo de tecla,
/// encendido en acento.
pub fn toggler(theme: &Theme, status: toggle::Status) -> toggle::Style {
    let c = chrome(theme);
    let is_toggled = match status {
        toggle::Status::Active { is_toggled } | toggle::Status::Hovered { is_toggled } => {
            is_toggled
        }
        toggle::Status::Disabled => false,
    };
    let (bg, fg) = if is_toggled {
        (c.accent, INK_ON_ACCENT)
    } else {
        (c.keycap, c.secondary)
    };
    toggle::Style {
        background: bg,
        background_border_width: 1.0,
        background_border_color: if is_toggled { c.accent } else { c.border },
        foreground: fg,
        foreground_border_width: 0.0,
        foreground_border_color: Color::TRANSPARENT,
    }
}

/// Punto de estado del marco (color del micrófono: el punto solo no dice nada,
/// siempre va con su texto al lado).
pub fn dot(color: Color) -> container::Style {
    container::Style {
        text_color: None,
        background: Some(Background::Color(color)),
        border: border(Color::TRANSPARENT, 0.0, 3.0),
        shadow: Shadow::default(),
    }
}

/// Barra del medidor de nivel del micrófono (3 px de ancho, esquina 2).
pub fn level_bar(theme: &Theme) -> container::Style {
    let c = chrome(theme);
    container::Style {
        text_color: None,
        background: Some(Background::Color(c.faint)),
        border: border(Color::TRANSPARENT, 0.0, 2.0),
        shadow: Shadow::default(),
    }
}

/// Editor del texto dictado sobre la tarjeta (`Editable text` del diseño): sin
/// fondo propio ni borde en reposo — el texto vive en la tarjeta. El foco se
/// sigue viendo (1.5 px de acento), que es obligatorio por accesibilidad.
pub fn transcript_editor(theme: &Theme, status: editor::Status) -> editor::Style {
    let c = chrome(theme);
    let (border_color, border_width) = match status {
        editor::Status::Focused => (c.accent, 1.5),
        _ => (Color::TRANSPARENT, 0.0),
    };
    editor::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: border(border_color, border_width, RADIUS_SM),
        icon: c.faint,
        placeholder: c.faint,
        value: c.text,
        selection: with_alpha(c.accent, 0.35),
    }
}

/// Tarjeta base: fondo + borde suave + esquina de tarjeta del diseño.
pub fn card(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        text_color: Some(t.text),
        background: Some(Background::Color(t.surface)),
        border: border(t.border, 1.0, RADIUS_LG),
        shadow: Shadow::default(),
    }
}

/// Tarjeta sin borde (zonas planas, por ejemplo el texto dictado).
pub fn flat(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        text_color: Some(t.text),
        background: Some(Background::Color(t.surface)),
        border: border(Color::TRANSPARENT, 0.0, RADIUS_MD),
        shadow: Shadow::default(),
    }
}

/// Botón principal (la acción importante): fondo de acento violeta y texto con
/// la tinta medida para AA sobre el acento (ver `INK_ON_ACCENT`: usando el
/// fondo del modo, en claro se quedaba en ~3.5:1).
pub fn primary(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let bg = match status {
        button::Status::Disabled => t.surface_alt,
        button::Status::Hovered | button::Status::Pressed => t.accent_hover,
        button::Status::Active => t.accent,
    };
    let fg = if matches!(status, button::Status::Disabled) {
        chrome(theme).faint
    } else {
        INK_ON_ACCENT
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: fg,
        border: border(Color::TRANSPARENT, 0.0, RADIUS_SM),
        shadow: Shadow::default(),
    }
}

/// Relleno de los botones secundarios («Copiar», «Limpiar», …). Zona cómoda
/// para el dedo (UI-FUTURA P1): el relleno de serie de iced deja los botones
/// demasiado bajos para un toque rápido. 8 px arriba/abajo y 16 a los lados.
pub const SECONDARY_BUTTON_PADDING: [f32; 2] = [SPACE_2 as f32, SPACE_3 as f32];

/// Relleno interior común de los campos (`text_input` y `pick_list`): los dos
/// con el mismo margen para que el texto empiece en la misma x (iced trae 10 px
/// a la izquierda en el `pick_list` y 5 en el `text_input`, y se notaba
/// desalineado).
///
/// Valores del diseño (nodos `3:560`/`3:660`): desplegables de **36 px de
/// alto** con **12 px** de margen lateral. Con texto de 13 px, 10 px arriba y
/// abajo dan esos 36 px; con el relleno viejo (5/10) los campos de Ajustes se
/// veían aplastados (~27 px) y con el texto pegado al borde izquierdo.
pub const FIELD_PADDING: Padding = Padding {
    top: 10.0,
    right: 12.0,
    bottom: 10.0,
    left: 12.0,
};

/// Hueco que se deja a la derecha en cada fila de Ajustes para el texto de
/// apoyo del campo (tamaño del modelo, ayuda del idioma) y para el botón de
/// borrar de las listas. Con el MISMO hueco en todas las filas, campos,
/// botones y «✕» acaban en la misma x (una sola columna derecha).
pub const AUX_COL_WIDTH: f32 = 64.0;

/// Botón secundario (acciones discretas): transparente, texto tenue → texto
/// normal al pasar por encima. `Disabled` (un botón sin `on_press`): texto aún
/// más apagado, para que se VEA que no se puede usar (iced da
/// `Status::Disabled` cuando el botón no tiene mensaje al pulsar).
pub fn secondary(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let (bg, fg) = match status {
        button::Status::Disabled => (None, chrome(theme).faint),
        button::Status::Hovered | button::Status::Pressed => (Some(t.surface_alt), t.text),
        button::Status::Active => (None, t.dim),
    };
    button::Style {
        background: bg.map(Background::Color),
        text_color: fg,
        border: border(Color::TRANSPARENT, 0.0, RADIUS_SM),
        shadow: Shadow::default(),
    }
}

/// Pestaña de sección (la usan Ajustes y la ventana principal). La activa lleva
/// fondo `surface_alt`, texto de acento y borde de 2 px; las demás son el botón
/// secundario de siempre. El punto `●` de la etiqueta lo pone la barra de
/// pestañas: la sección activa se distingue sin depender solo del color
/// (accesibilidad, R7).
pub fn tab(theme: &Theme, status: button::Status, active: bool) -> button::Style {
    if !active {
        return secondary(theme, status);
    }
    let t = tokens(theme);
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => t.surface,
        _ => t.surface_alt,
    };
    let border_color = match status {
        button::Status::Active => t.accent,
        _ => t.border,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: t.accent_text,
        border: border(border_color, 2.0, RADIUS_SM),
        shadow: Shadow::default(),
    }
}

/// Pestaña de la navegación de Ajustes (`Settings navigation` del diseño): sin
/// caja propia; la activa en texto principal y el resto en secundario. La raya
/// de 2 px la pinta la interfaz con [`tab_underline`] (el diseño la cuelga del
/// borde de abajo de la pestaña, no de una caja).
pub fn settings_tab(theme: &Theme, status: button::Status, active: bool) -> button::Style {
    let c = chrome(theme);
    let text_color = match (active, status) {
        (true, _) => c.text,
        (false, button::Status::Hovered | button::Status::Pressed) => c.text,
        (false, _) => c.secondary,
    };
    button::Style {
        background: None,
        text_color,
        border: border(Color::TRANSPARENT, 0.0, 0.0),
        shadow: Shadow::default(),
    }
}

/// Fondo de la pestaña de navegación de Ajustes: solo la franja de 2 px del
/// borde INFERIOR, en el acento del diseño (nada si no es la activa). iced 0.13
/// no deja poner borde por lado (`Border` es un ancho único), así que la franja
/// la pinta el fondo con un degradado vertical de dos paradas pegadas abajo — a
/// `Radians(0.0)` el degradado va de abajo arriba, así que el offset 0 es el
/// borde inferior. Como es el fondo del propio contenedor, la raya mide justo
/// lo que mide la etiqueta.
pub fn tab_underline(theme: &Theme, active: bool, height: f32) -> Background {
    let accent = if active {
        chrome(theme).accent_active
    } else {
        Color::TRANSPARENT
    };
    let clear = with_alpha(accent, 0.0);
    let edge = (2.0 / height.max(1.0)).min(1.0);
    Background::Gradient(Gradient::Linear(
        Linear::new(Radians(0.0))
            .add_stop(0.0, accent)
            .add_stop(edge, accent)
            .add_stop((edge + 0.001).min(1.0), clear)
            .add_stop(1.0, clear),
    ))
}

/// Botón del micrófono (hero): transparente; el estado lo comunica la tarjeta
/// de dentro. El cursor de mano es el aviso de que se puede pulsar.
pub fn mic(theme: &Theme, _status: button::Status) -> button::Style {
    let t = tokens(theme);
    button::Style {
        background: None,
        text_color: t.text,
        border: border(Color::TRANSPARENT, 0.0, RADIUS_MD),
        shadow: Shadow::default(),
    }
}

/// Tarjeta del estado del micrófono: borde con el color del estado (tres
/// avisos a la vez: borde, punto y texto).
pub fn mic_card(theme: &Theme, state: Color) -> container::Style {
    let t = tokens(theme);
    container::Style {
        text_color: Some(t.text),
        background: Some(Background::Color(t.surface)),
        border: border(state, 1.0, RADIUS_MD),
        shadow: Shadow::default(),
    }
}

/// Campo de texto: fondo alterno y borde de acento al enfocar. El foco se
/// dibuja con 1.5 px de acento (accesibilidad: el foco del teclado tiene que
/// VERSE, no suponerse).
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

/// Editor de varias líneas del texto dictado (requisito R3): los mismos
/// colores que el campo de texto, con anillo de foco visible (1.5 px de acento).
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

/// Desplegable: los mismos colores que el campo de texto.
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

/// Casilla: fondo alterno y marca en acento.
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

/// Zona de desplazamiento: contenedor plano y barras discretas.
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

/// Línea separadora horizontal suave.
pub fn rule(theme: &Theme) -> rule::Style {
    rule::Style {
        color: tokens(theme).border,
        width: 1,
        radius: Radius::from(1.0),
        fill_mode: rule::FillMode::Full,
    }
}

/// Barra de progreso con color propio (nivel del micrófono / descarga).
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

    /// La raya de la pestaña activa es la franja de 2 px del borde INFERIOR:
    /// las dos primeras paradas del degradado caen en el offset 0 y en 2/alto, y
    /// a partir de ahí el color se apaga. Si alguien cambia el reparto, la raya
    /// deja de medir 2 px (o se pinta la caja entera).
    #[test]
    fn el_subrayado_de_pestana_es_la_franja_inferior_de_2_px() {
        let th = custom(true);
        let Background::Gradient(Gradient::Linear(linear)) = tab_underline(&th, true, 43.0) else {
            panic!("el subrayado debe ser un gradiente");
        };
        let stops: Vec<_> = linear.stops.iter().flatten().collect();
        assert_eq!(
            stops.len(),
            4,
            "dos paradas para la franja y dos para el resto"
        );
        assert_eq!(stops[0].offset, 0.0);
        assert_eq!(
            stops[0].color, DARK.accent_hover,
            "la franja va en el acento activo"
        );
        assert!((stops[1].offset - 2.0 / 43.0).abs() < 1e-6);
        assert_eq!(stops[1].color, stops[0].color, "la franja es maciza");
        assert!(
            stops[2].color.a < 0.01,
            "por encima de la franja, transparente"
        );

        // Si no es la activa, no se pinta nada.
        let Background::Gradient(Gradient::Linear(inactive)) = tab_underline(&th, false, 43.0)
        else {
            panic!("el fondo del inactivo también es un gradiente");
        };
        assert!(
            inactive
                .stops
                .iter()
                .flatten()
                .all(|stop| stop.color.a < 0.01),
            "la pestaña inactiva no lleva trazo"
        );
    }

    #[test]
    fn spacing_sigue_la_grid_de_8px() {
        for s in [SPACE_2, SPACE_3, SPACE_4, SPACE_5, SPACE_6] {
            assert_eq!(s % 8, 0, "spacing {s} debe ser múltiplo de 8");
        }
    }

    /// La rejilla de las 5 pestañas cuadra con la pantalla `3:604`: los 916 px
    /// de contenido = margen 20 + rótulo 606 + hueco 20 + control 250 + margen
    /// 20. Si alguien mueve una pieza, la fila deja de cuadrar con el diseño y el
    /// test lo avisa antes de que se vea descuadrado en la interfaz.
    #[test]
    fn la_rejilla_de_ajustes_cuadra_con_el_frame_3604() {
        assert_eq!(
            GRID_ROW_PAD_H
                + GRID_LABEL_WIDTH
                + GRID_ROW_PAD_H
                + GRID_CONTROL_WIDTH
                + GRID_ROW_PAD_H,
            GRID_CONTENT_WIDTH,
            "rótulo + control + insets deben sumar el contenido de 916 px"
        );
        // El interruptor (36×22, nodo 3:692) cae DENTRO de la columna de
        // control de 250 y comparte su borde derecho (x=214 dentro de la caja).
        // iced lo pinta 8 px más ancho (44 = size×2): se deja así y se deja
        // anotado; lo que fija la rejilla es el borde derecho.
        assert_eq!(GRID_TOGGLE_WIDTH, 36.0, "nodo 3:692");
        assert_eq!(GRID_TOGGLE_SIZE, 22.0, "nodo 3:692");
        assert_eq!(GRID_CONTROL_WIDTH - GRID_TOGGLE_WIDTH, 214.0);
        // Fila: 15 + 38 (rótulo + ayuda) + 15 = 68 (nodo 3:656).
        assert_eq!(GRID_ROW_PAD_V * 2.0 + 38.0, GRID_ROW_HEIGHT);
        // Pestañas y separaciones del diseño.
        assert_eq!(GRID_TAB_HEIGHT, 43.0, "nodo 3:642");
        assert_eq!(GRID_TAB_GAP, 28.0, "x=0 → 113, nodo 3:642");
        assert_eq!(GRID_SECTION_GAP, 20.0, "nodos 3:653 → 3:670");
        assert_eq!(GRID_SECTION_TITLE_GAP, 10.0, "y=0 → 25, nodo 3:654");
    }

    /// Un botón sin acción (iced entrega `Status::Disabled`) no puede verse
    /// igual que uno activo: por coherencia de estado.
    #[test]
    fn el_boton_deshabilitado_se_ve_distinto() {
        reset_palette_override();
        let th = custom(true);
        let sec_act = secondary(&th, button::Status::Active);
        let sec_dis = secondary(&th, button::Status::Disabled);
        assert_ne!(sec_act.text_color, sec_dis.text_color);
        assert_eq!(sec_dis.text_color, chrome(&th).faint);
        assert!(
            sec_dis.background.is_none(),
            "el secundario deshabilitado no lleva relleno"
        );
        let pri_act = primary(&th, button::Status::Active);
        let pri_dis = primary(&th, button::Status::Disabled);
        assert_ne!(
            format!("{:?}", pri_act.background),
            format!("{:?}", pri_dis.background),
            "el primario deshabilitado no puede llevar el relleno de acento"
        );
        assert_ne!(pri_act.text_color, pri_dis.text_color);
    }

    #[test]
    fn radios_dentro_de_8_12px() {
        assert!(RADIUS_SM >= 8.0 && RADIUS_SM <= 10.0);
        assert!(RADIUS_MD >= 8.0 && RADIUS_MD <= 12.0);
        // Del diseño: tarjeta 12, tecla 5.
        assert_eq!(RADIUS_LG, 12.0);
        assert_eq!(RADIUS_KEYCAP, 5.0);
        assert_eq!(RADIUS_CIRCLE, RECORD_BUTTON_SIZE / 2.0);
    }

    #[test]
    fn design_type_factor_1_es_la_escala_medida_en_figma() {
        let ty = design_type(1.0);
        assert_eq!(ty.brand_sub, 9);
        assert_eq!(ty.badge, 10);
        assert_eq!(ty.chrome, 11);
        assert_eq!(ty.body, 12);
        assert_eq!(ty.control, 13);
        assert_eq!(ty.nav, 14);
        assert_eq!(ty.brand, 18);
        assert_eq!(ty.transcript, 19);
        assert_eq!(ty.hero, 22);
        assert_eq!(ty.title, 28);
        // El orden se respeta: crece con el factor base y nunca baja (con 0.85
        // los tamaños pequeños se redondean y dos escalones seguidos pueden
        // coincidir: 9→8 y 10→9).
        for f in [0.85, 1.0, 1.15, 1.3] {
            let t = design_type(f);
            let cadena = [
                t.brand_sub,
                t.badge,
                t.chrome,
                t.body,
                t.control,
                t.nav,
                t.brand,
                t.transcript,
                t.hero,
                t.title,
            ];
            assert!(
                cadena.windows(2).all(|w| w[0] <= w[1]),
                "jerarquía rota con factor {f}: {cadena:?}"
            );
            assert!(cadena.iter().all(|v| (8..=64).contains(v)));
            assert!(
                cadena[0] < cadena[9],
                "el título tiene que quedar por encima del subtítulo del wordmark ({f})"
            );
        }
    }

    #[test]
    fn chrome_resuelve_por_modo_y_sigue_la_paleta_del_usuario() {
        reset_palette_override();
        let dark = chrome(&custom(true));
        assert_eq!(dark.sidebar, SIDEBAR);
        assert_eq!(dark.window_bar, WINDOW_BAR);
        assert_eq!(dark.keycap, KEYCAP);
        assert_eq!(dark.nav_active, NAV_ACTIVE);
        assert_eq!(dark.divider, DIVIDER);
        assert_eq!(dark.faint, FAINT);
        // Del diseño: tarjeta y campo salen de la paleta de colores.
        assert_eq!(dark.card, SURFACE);
        assert_eq!(dark.field, SURFACE_ALT);
        assert_eq!(dark.border, BORDER);
        assert_eq!(dark.text, TEXT);
        assert_eq!(dark.secondary, DIM);
        assert_eq!(dark.accent, ACCENT);
        assert_eq!(dark.accent_active, ACCENT_HOVER);

        let light = chrome(&custom(false));
        assert_ne!(
            light.sidebar, dark.sidebar,
            "el chrome también tiene modo claro"
        );
        assert_eq!(light.sidebar, SIDEBAR_LIGHT);
        assert_eq!(light.card, SURFACE_LIGHT);

        // Un cambio del usuario llega al marco (marca y textos).
        let cfg = Config {
            palette: Some(PaletteConfig {
                accent: Some("#00AAFF".into()),
                text: Some("#FFFFFF".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let p = resolve_palette(&cfg, true);
        let _scope = PaletteOverrideScope::new(p);
        let custom_chrome = chrome(&custom(true));
        assert_eq!(custom_chrome.accent, parse_hex("#00AAFF").unwrap());
        assert_eq!(custom_chrome.text, parse_hex("#FFFFFF").unwrap());
    }

    /// El texto tenue del diseño (#7F808D) NO pasa AA sobre la tarjeta ni sobre
    /// el campo: se anota el contraste medido (la tarea pide dejarlo apuntado, no
    /// cambiarlo por cuenta propia). Si alguien «arregla» el color sin tocar este
    /// test, el número deja de cuadrar.
    #[test]
    fn faint_del_diseno_registra_su_ratio_medido() {
        let sobre_fondo = contrast_ratio(FAINT, BG);
        let sobre_tarjeta = contrast_ratio(FAINT, SURFACE);
        let sobre_campo = contrast_ratio(FAINT, SURFACE_ALT);
        let sobre_sidebar = contrast_ratio(FAINT, SIDEBAR);
        assert!(
            sobre_fondo >= 4.5 && sobre_sidebar >= 4.5,
            "fondo {sobre_fondo:.2} / sidebar {sobre_sidebar:.2}"
        );
        assert!(
            (4.1..4.5).contains(&sobre_tarjeta),
            "se esperaba < AA sobre tarjeta, mide {sobre_tarjeta:.2}"
        );
        assert!(
            (4.3..4.5).contains(&sobre_campo),
            "se esperaba < AA sobre campo, mide {sobre_campo:.2}"
        );
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
        // Se asegura de que ningún test anterior dejó cambios puestos en el
        // hilo. Sin esto, la paleta de otro test podría estropear las mediciones
        // de contraste WCAG.
        clear_palette_override();
        let dark_theme = custom(true);
        assert!(is_dark(&dark_theme));
        assert_eq!(*tokens(&dark_theme), DARK);
        let light_theme = custom(false);
        assert!(!is_dark(&light_theme));
        assert_eq!(*tokens(&light_theme), LIGHT);
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
        // 0.85 (pequeña): todo más pequeño, pero el orden se mantiene.
        let small = font_sizes(0.85);
        assert!(small.sm < FONT_SM);
        assert!(small.md < FONT_MD);
        // 1.3 (muy grande): todo más grande.
        let large = font_sizes(1.3);
        assert!(large.md > FONT_MD);
        assert!(large.hero > FONT_HERO);
        // El orden se guarda: cada nivel supera al anterior con cualquier factor.
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
        // Factores extremos nunca se salen de 8..=64 px.
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

    // ── Contraste WCAG medido (requisito R4: medido, no estimado) ───────────
    //
    // Se miden los pares REALES de la interfaz: cada color de texto contra el
    // fondo de la ventana Y contra la tarjeta (los textos están en los dos). Si
    // un test falla, se ajusta el color y se vuelve a medir: cambiar un color
    // sin pasar por aquí no vale.

    /// AA para texto normal (≥ 4.5:1) en todos los colores que se usan como texto.
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

    /// AA para texto grande y piezas gráficas (≥ 3:1): el violeta de marca como
    /// texto GRANDE (hero/spinner, ≥ 24 px) y la tinta de los botones llenos de
    /// acento (que sí es texto normal → pide 4.5).
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
            // Texto del botón principal: tinta fija sobre relleno de acento
            // (y sobre su hover, que también es un fondo posible).
            let sobre_acento = contrast_ratio(INK_ON_ACCENT, p.accent);
            let sobre_hover = contrast_ratio(INK_ON_ACCENT, p.accent_hover);
            assert!(
                sobre_acento >= 4.5 && sobre_hover >= 4.5,
                "{nombre}: tinta sobre acento {sobre_acento:.2}:1 / hover {sobre_hover:.2}:1"
            );
        }
    }

    /// Comprobación del cálculo contra los valores publicados de WCAG: negro
    /// sobre blanco = 21:1 exacto; #767676 sobre blanco = 4.54:1 (el gris mínimo
    /// AA que cita el W3C).
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

    /// La tinta de los botones y el acento para texto son colores
    /// independientes del modo: cambiar de tema no puede romper el contraste
    /// por descuido.
    #[test]
    fn tokens_de_acceso_no_cambian_con_el_modo() {
        assert_eq!(DARK.accent_text, ACCENT_TEXT);
        assert_eq!(LIGHT.accent_text, ACCENT_TEXT_LIGHT);
        assert_eq!(INK_ON_ACCENT, rgb!(0x17, 0x18, 0x1C));
    }

    // ── Personalización II: paleta editable ──────────────────────────────────

    /// Quita los cambios antes de cada test que mide colores de serie (lo
    /// guardado en el hilo puede sobrevivir entre tests si una comprobación
    /// falla a mitad).
    fn reset_palette_override() {
        clear_palette_override();
    }

    #[test]
    fn parse_hex_acepta_rrggbb_y_rgb_y_tolerancias() {
        reset_palette_override();
        // #RRGGBB en mayúsculas y minúsculas.
        let c1 = parse_hex("#FF8800").unwrap();
        assert_eq!(c1.r, 1.0);
        assert!((c1.g - 136.0 / 255.0).abs() < 0.001);
        assert_eq!(c1.b, 0.0);
        // #RGB se pasa a #RRGGBB repitiendo cada dígito.
        let c2 = parse_hex("#f0a").unwrap();
        assert_eq!(c2.r, 1.0); // f → ff
        assert_eq!(c2.g, 0.0); // 0 → 00
        assert!((c2.b - 170.0 / 255.0).abs() < 0.001); // a → aa
                                                        // Sin `#`, en minúsculas.
        let c3 = parse_hex("7c6cff").unwrap();
        assert_eq!(c3.r, 0x7C as f32 / 255.0);
        assert_eq!(c3.g, 0x6C as f32 / 255.0);
        assert_eq!(c3.b, 0xFF as f32 / 255.0);
        // Espacios de alrededor.
        let c4 = parse_hex("  #ABCDEF  ").unwrap();
        assert_eq!(c4.r, 0xAB as f32 / 255.0);
        // También se acepta el prefijo 0x.
        let c5 = parse_hex("0x101010").unwrap();
        assert_eq!(c5.r, 16.0 / 255.0);
    }

    #[test]
    fn parse_hex_rechaza_basura_sin_panicar() {
        reset_palette_override();
        // Longitudes que no se aceptan.
        assert!(parse_hex("").is_none());
        assert!(parse_hex("#").is_none());
        assert!(parse_hex("#FF").is_none());
        assert!(parse_hex("#FFFF").is_none()); // 4 = ARGB hex CSS, NO lo aceptamos
        assert!(parse_hex("#FFFFFFFF").is_none()); // 8 dígitos con alpha no es editable
                                                    // Caracteres que no son hex.
        assert!(parse_hex("#GGGGGG").is_none());
        assert!(parse_hex("#XX00FF").is_none());
        assert!(parse_hex("no es hex").is_none());
        assert!(parse_hex("##").is_none());
        // NUNCA rompe; la interfaz puede llamar aquí sin protegerse.
        let opts = [
            "",
            "#",
            "#FF",
            "#FFFF",
            "#FFFFFFFF",
            "#GGGGGG",
            "no es hex",
            "##",
            "  ",
            "0xZZ",
            "café",
        ];
        for opt in opts {
            assert!(
                parse_hex(opt).is_none(),
                "parse_hex({opt:?}) debería ser None"
            );
        }
    }

    #[test]
    fn resolve_palette_sin_override_es_identica_al_default_del_modo() {
        reset_palette_override();
        let cfg = Config::default();
        let d = resolve_palette(&cfg, true);
        let l = resolve_palette(&cfg, false);
        assert_eq!(d, DARK, "sin override oscuro = DARK");
        assert_eq!(l, LIGHT, "sin override claro = LIGHT");

        // Un config con palette = Some(PaletteConfig::default()) (todo None)
        // también acaba en la paleta de serie entera.
        let cfg_empty = Config {
            palette: Some(PaletteConfig::default()),
            ..Default::default()
        };
        assert_eq!(resolve_palette(&cfg_empty, true), DARK);
        assert_eq!(resolve_palette(&cfg_empty, false), LIGHT);
    }

    #[test]
    fn resolve_palette_mezcla_por_campo_y_cae_a_default_si_hex_invalido() {
        reset_palette_override();
        let cfg = Config {
            palette: Some(PaletteConfig {
                accent: Some("#00AAFF".into()),
                surface_alt: Some("#222".into()),
                text: Some("BASURA".into()), // hex inválido → default
                ..Default::default()
            }),
            ..Default::default()
        };
        let p = resolve_palette(&cfg, true);
        // Solo cambiaron accent y surface_alt.
        assert_eq!(p.accent, parse_hex("#00AAFF").unwrap());
        assert_eq!(p.surface_alt, parse_hex("#222").unwrap());
        // El resto se queda en los colores oscuros de serie.
        assert_eq!(p.bg, DARK.bg);
        assert_eq!(p.surface, DARK.surface);
        assert_eq!(p.text, DARK.text, "hex inválido cae al default");
        assert_eq!(p.err, DARK.err);

        // En modo claro se usan los colores claros de serie en lo que no se cambió.
        let l = resolve_palette(&cfg, false);
        assert_eq!(l.accent, parse_hex("#00AAFF").unwrap());
        assert_eq!(l.surface_alt, parse_hex("#222").unwrap());
        assert_eq!(l.bg, LIGHT.bg);
        assert_eq!(l.text, LIGHT.text);
    }

    #[test]
    fn palette_override_se_aplica_a_tokens_entre_set_y_clear() {
        reset_palette_override();
        let cfg = Config {
            palette: Some(PaletteConfig {
                accent: Some("#00FF00".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let p = resolve_palette(&cfg, true);
        // Antes del bloque: `tokens` da la paleta oscura (sin cambios).
        let theme_default = custom(true);
        assert_eq!(*tokens(&theme_default), DARK);
        // Dentro del bloque: `tokens` devuelve la paleta resuelta.
        let _scope = PaletteOverrideScope::new(p);
        assert_eq!(tokens(&theme_default).accent, parse_hex("#00FF00").unwrap());
        // Al soltar el bloque: vuelve a la oscura.
        drop(_scope);
        assert_eq!(*tokens(&theme_default), DARK);
    }

    #[test]
    fn palette_to_config_puede_roundtrip_con_parse_hex() {
        reset_palette_override();
        let original = DARK;
        let cfg = palette_to_config(original);
        let resolved = resolve_palette(
            &Config {
                palette: Some(cfg.clone()),
                ..Default::default()
            },
            true,
        );
        // Cada campo tiene que coincidir con el original (con un margen por el
        // redondeo de f32 → u8 → f32).
        fn approx(a: Color, b: Color) -> bool {
            (a.r - b.r).abs() < 0.01 && (a.g - b.g).abs() < 0.01 && (a.b - b.b).abs() < 0.01
        }
        for (field, (orig, res)) in [
            ("bg", (original.bg, resolved.bg)),
            ("surface", (original.surface, resolved.surface)),
            ("surface_alt", (original.surface_alt, resolved.surface_alt)),
            ("text", (original.text, resolved.text)),
            ("dim", (original.dim, resolved.dim)),
            ("accent", (original.accent, resolved.accent)),
            (
                "accent_hover",
                (original.accent_hover, resolved.accent_hover),
            ),
            ("accent_text", (original.accent_text, resolved.accent_text)),
            ("ok", (original.ok, resolved.ok)),
            ("warn", (original.warn, resolved.warn)),
            ("err", (original.err, resolved.err)),
            ("border", (original.border, resolved.border)),
        ] {
            assert!(approx(orig, res), "{field}: {orig:?} vs {res:?}");
        }
        // Forma del hex: #RRGGBB en mayúsculas.
        assert_eq!(cfg.bg.as_deref(), Some("#17181C"));
    }

    #[test]
    fn presets_tienen_cinco_opciones_y_cubre_oscuro_y_claro() {
        reset_palette_override();
        assert_eq!(PalettePreset::ALL.len(), 5);
        // Cada preset tiene una rama oscura y una clara.
        for preset in PalettePreset::ALL {
            let _ = preset.colors(true);
            let _ = preset.colors(false);
        }
    }

    #[test]
    fn aviso_de_contraste_segun_wcag() {
        reset_palette_override();
        // Texto casi del mismo color que el fondo: contraste < 3:1 → no se lee.
        let bajo = Color::from_rgb(0.5, 0.5, 0.5);
        let fondo = Color::from_rgb(0.5, 0.5, 0.5);
        let ratio = contrast_ratio(bajo, fondo);
        assert!(
            ratio < 3.0,
            "mismo gris sobre mismo gris debe ser <3:1, midió {ratio:.2}"
        );
        // Combinación que se lee: blanco sobre negro.
        assert!(contrast_ratio(Color::WHITE, Color::BLACK) >= 3.0);
        // Texto blanco sobre fondo blanco: contraste = 1:1 (no se lee).
        assert!(contrast_ratio(Color::WHITE, Color::WHITE) < 3.0);
        // Texto blanco sobre gris claro: contraste ≤ 3:1 → NO se lee.
        let gris_claro = rgb!(0xCC, 0xCC, 0xCC);
        assert!(
            contrast_ratio(Color::WHITE, gris_claro) < 3.0,
            "blanco sobre #CCCCCC debe ser <3:1"
        );
    }

    #[test]
    fn contrast_ratio_dispara_aviso_si_text_o_dim_bajan_de_3_sobre_bg() {
        reset_palette_override();
        // Ayudante: la interfaz enseña el aviso cuando text o dim bajan de 3:1
        // sobre el fondo de la ventana (pedido de UX: «no negociable»).
        fn legible(c: Color, bg: Color) -> bool {
            contrast_ratio(c, bg) >= 3.0
        }
        // Los colores de serie: SIEMPRE se leen.
        assert!(legible(DARK.text, DARK.bg));
        assert!(legible(DARK.dim, DARK.bg));
        assert!(legible(LIGHT.text, LIGHT.bg));
        assert!(legible(LIGHT.dim, LIGHT.bg));
        // Cambio absurdo: texto casi del mismo color que el fondo.
        let cfg = Config {
            palette: Some(PaletteConfig {
                text: Some("#181818".into()), // 1 px sobre el #17181C de DARK.bg → ilegible
                ..Default::default()
            }),
            ..Default::default()
        };
        let p = resolve_palette(&cfg, true);
        assert!(
            !legible(p.text, p.bg),
            "override patológico debe disparar el aviso"
        );
    }
}
