//! Imágenes del usuario y animación por fotogramas (Personalización III).
//!
//! El usuario puede elegir:
//!
//! - Una **imagen de fondo** para la ventana principal.
//! - Una **imagen fija** O una **secuencia de PNG** para la burbuja flotante
//!   (la animación va cambiando de imagen a mano: iced 0.13 no trae nada para
//!   GIF/WebP animado, y pasar a iced 0.14 —que sí lo trae— queda fuera).
//!
//! Reglas (comprobadas con tests):
//!
//! 1. **Las rutas van en el config, nunca escritas en el código**. La ruta
//!    sale de config.json (`config.bg_image_path`, `config.bubble_image_path`,
//!    `config.bubble_frames_path`). Si el fichero no existe o no se puede leer,
//!    se vuelve al aspecto de siempre (obligatorio: nunca una ventana en negro
//!    ni un cierre).
//! 2. **Animación sin GIF**: la lista de imágenes se ordena por nombre
//!    y se avanza con `iced::time::every` cada `1000/fps` ms. El índice da
//!    la vuelta sin salirse. Si la carpeta está vacía, se vuelve a la
//!    imagen fija.
//! 3. **Apagarlo del todo es normal**: rutas en blanco = aspecto original. El
//!    botón «Quitar» vacía la ruta que toque en config.json (sin tocar la otra).
//!
//! ## Cómo se cargan las imágenes
//!
//! En iced 0.13, `widget::image::Handle::from_path` pide una ruta que exista
//! en el disco (los bytes se leen al dibujar, no al crear el Handle). Por eso
//! se comprueba aquí ANTES de crear el Handle: si la ruta no existe o no es un
//! fichero normal → `None` (quien pinta pone el aspecto de siempre). El Handle
//! no se guarda en memoria porque hay que poder recargar en caliente (elegir
//! otra vez el mismo fichero tiene que verse al momento).

use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Extensiones que se aceptan para imágenes (en minúsculas, sin el punto).
/// Son las que entiende el crate `image` 0.25 (PNG, JPEG, BMP, GIF, WebP,
/// TIFF…). Lo que iced 0.13 puede decodificar depende de las opciones del
/// crate `image`; la opción `image` de iced activa `image/default`, que trae
/// los formatos habituales.
const SUPPORTED_EXTS: &[&str] = &["png", "jpg", "jpeg", "bmp", "gif", "webp", "tiff"];

/// ¿La extensión del fichero está en `SUPPORTED_EXTS`?
/// Se compara en minúsculas; sin extensión → falso.
pub fn is_supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .map(|s| SUPPORTED_EXTS.iter().any(|e| *e == s))
        .unwrap_or(false)
}

/// ¿La ruta sirve como imagen de fondo o fija?
///
/// Tiene que cumplir todo esto para que el widget de iced no falle:
/// - La ruta existe y es un fichero normal (no una carpeta).
/// - La extensión está en `SUPPORTED_EXTS`.
///
/// Esto NO abre el fichero (eso lo hace iced al dibujar); comprobar una vez
/// más por imagen sale casi gratis porque solo se llama al cambiar la ruta en
/// Ajustes o al montar `Loaded::current_frame` cada vez que se recarga.
pub fn is_loadable(path: &Path) -> bool {
    path.is_file() && is_supported_extension(path)
}

/// Resuelve la ruta guardada en el config:
///
/// - `None`, vacía o solo espacios → `None` (sin imagen).
/// - Si la ruta existe y tiene extensión aceptada, se devuelve limpia
///   (`canonicalize` cuando se puede; si falla por permisos o porque el
///   diálogo de archivos acaba de escribirla, se devuelve la original).
///
/// Devolver `None` siempre significa "se pinta el aspecto de siempre".
pub fn resolve_static(path: Option<&str>) -> Option<PathBuf> {
    let trimmed = path.map(str::trim).filter(|s| !s.is_empty())?;
    let p = PathBuf::from(trimmed);
    if !is_loadable(&p) {
        return None;
    }
    // `canonicalize` falla si la ruta no existe (ahí `is_loadable` ya habría
    // dicho que no), pero también con problemas de permisos o con rutas muy
    // largas en Windows. La original le vale a iced igual: limpiarla es solo
    // para que el log quede bonito.
    Some(std::fs::canonicalize(&p).unwrap_or(p))
}

/// Lista las imágenes de una carpeta, ordenadas por nombre.
///
/// - `None` o vacía → `None` (sin animación; quien pinta usa la imagen fija).
/// - Carpeta que no existe → `None`.
/// - Carpeta vacía o sin extensiones aceptadas → `None`.
/// - Con al menos una imagen válida → `Some(vec)` con las rutas en orden.
///
/// Se descartan carpetas, ficheros sin extensión aceptada y entradas que fallen
/// `is_loadable` (por ejemplo enlaces rotos). El orden es de menor a mayor
/// nombre con `to_string_lossy`: "frame_10.png" va antes que "frame_2.png"
/// (orden de texto puro); se da por hecho que el usuario nombra los ficheros
/// con ceros a la izquierda.
pub fn resolve_frames(dir: Option<&str>) -> Option<Vec<PathBuf>> {
    let trimmed = dir.map(str::trim).filter(|s| !s.is_empty())?;
    let p = PathBuf::from(trimmed);
    if !p.is_dir() {
        return None;
    }
    let mut frames: Vec<PathBuf> = match std::fs::read_dir(&p) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|path| is_loadable(path))
            .collect(),
        Err(_) => return None,
    };
    if frames.is_empty() {
        return None;
    }
    frames.sort_by(|a, b| {
        a.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
            .cmp(
                &b.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
            )
    });
    Some(frames)
}

/// Avanza el índice de imagen N puestos (1 en el uso normal) y devuelve el
/// nuevo, dando la vuelta. Si `frames_len` es 0 → 0; un `current` fuera de
/// rango se lleva a 0 antes de avanzar. Función suelta (sin estado) para
/// poder probarla sin arrancar iced.
pub fn next_frame_index(current: usize, frames_len: usize) -> usize {
    if frames_len == 0 {
        return 0;
    }
    let cur = current.min(frames_len - 1);
    // `saturating_add`: si `current` fuera `usize::MAX`, no se desborda;
    // se suma 1 y se da la vuelta con el módulo.
    (cur + 1) % frames_len
}

/// Imágenes por segundo por defecto para la animación. Rápido para que la
/// imagen se sienta viva sin gastar CPU (~16 ms por vuelta → ~60 Hz). El
/// usuario puede bajarlo (1..=30).
pub const DEFAULT_BUBBLE_FPS: u8 = 12;

/// Imágenes por segundo máximas. Por encima, la animación se vuelve
/// inmanejable en pantallas normales y el gasto de CPU y disco en el `Poll` no
/// compensa. 30 es el techo clásico de los sprites 2D.
pub const MAX_BUBBLE_FPS: u8 = 30;

/// Mínimo aceptado. Por debajo de 1 no se nota la animación; un 0 rompería la
/// división de `interval_from_fps`. Se usa 1 como suelo.
pub const MIN_BUBBLE_FPS: u8 = 1;

/// Comprueba y ajusta las imágenes por segundo que elige el usuario. Si se sale
/// del rango, se usa el valor por defecto. Función suelta, se puede probar.
pub fn clamp_fps(value: u8) -> u8 {
    if value < MIN_BUBBLE_FPS || value > MAX_BUBBLE_FPS {
        DEFAULT_BUBBLE_FPS
    } else {
        value
    }
}

/// Calcula cuánto hay que esperar entre vueltas a partir de un valor ya
/// validado. Función suelta; 0 imágenes por segundo cae al valor por defecto
/// (caso raro: quien llama debería haber pasado antes por `clamp_fps`).
pub fn interval_from_fps(fps: u8) -> std::time::Duration {
    let fps = clamp_fps(fps).max(1);
    std::time::Duration::from_millis(1_000_u64 / u64::from(fps))
}

/// ── GIF animado de un solo fichero (último sprint) ────────────────────────
///
/// La burbuja acepta UN fichero: PNG/JPG/WebP fijo o GIF animado. iced 0.13
/// solo pinta el primer fotograma del GIF, así que aquí se sacan todos con el
/// crate `image` y se guardan como RGBA en memoria (la animación avanza con el
/// mismo `AnimationTick` que las carpetas de siempre). Un GIF roto o de un
/// solo fotograma → `None`/imagen fija.

/// ¿La ruta es un `.gif`? Solo mira la extensión (no abre el fichero).
pub fn is_gif_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|s| s.eq_ignore_ascii_case("gif"))
        .unwrap_or(false)
}

/// Un fotograma de GIF ya decodificado: tamaño, píxeles RGBA y espera en ms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GifFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub delay_ms: u64,
}

/// Saca TODOS los fotogramas de un GIF como RGBA en memoria.
/// `None` si el fichero no existe, no es un GIF o está roto.
/// Tope de 256 fotogramas y 512×512 por fotograma para no reventar la memoria
/// con un GIF absurdo (una burbuja no necesita más).
pub fn decode_gif_frames(path: &std::path::Path) -> Option<Vec<GifFrame>> {
    use image::AnimationDecoder;
    if !path.is_file() || !is_gif_path(path) {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    let cursor = std::io::Cursor::new(bytes);
    let decoder = image::codecs::gif::GifDecoder::new(cursor).ok()?;
    let mut out = Vec::new();
    for frame in decoder.into_frames().take(256) {
        let frame = frame.ok()?;
        let (w, h) = (frame.buffer().width(), frame.buffer().height());
        if w == 0 || h == 0 || w > 512 || h > 512 {
            return None;
        }
        let delay = frame.delay();
        let delay_ms = u64::from(delay.numer_denom_ms().0).max(20);
        out.push(GifFrame {
            width: w,
            height: h,
            rgba: frame.into_buffer().into_raw(),
            delay_ms,
        });
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
}

/// Mensaje del `Poll` que se traduce a `Message::AnimationTick` (en `app.rs`)
/// cuando hay animación. Es `pub` para que el módulo que arma la suscripción
/// (`app::WhisprApp::subscription`) pueda montar el `map` con esta misma forma.
pub fn tick_to_instant(_instant: std::time::Instant) -> std::time::Instant {
    // Envoltorio sencillo; el `Poll` de verdad vive en `app.rs` y solo
    // necesita poder hacer `.map(Message::AnimationTick)` sobre la suscripción
    // de tiempo. Dejar el paso por aquí permite cambiar lo del tick (por
    // ejemplo detectar atascos) sin tocar el reparto de mensajes de la interfaz.
    std::time::Instant::now()
}

/// ── Guardar los ficheros del usuario (H4 del QA 24/09/2026) ───────────────
///
/// El usuario elegía el fondo o las imágenes desde `Descargas` y el config
/// guardaba ESA ruta: al vaciar Descargas, la personalización desaparecía sin
/// avisar. Ahora `import_asset` / `import_frames_dir` copian el material a
/// `%APPDATA%/whispr-open/assets/` y el config guarda la copia, así que aguanta
/// que se limpien carpetas temporales.
///
/// Compatibilidad: un config viejo con una ruta de fuera que SIGUE existiendo
/// se respeta tal cual (no se cambia a la fuerza); solo se borra del config
/// cuando apunta a algo que ya no está (`sanitize_config_assets`).

/// Carpeta de datos de la app (`%APPDATA%/whispr-open/`): la misma del log
/// (`crate::log::log_dir`), para que una copia de esa carpeta lo tenga todo.
pub fn app_data_dir() -> PathBuf {
    crate::log::log_dir()
}

/// Carpeta donde la app guarda copia de los ficheros elegidos:
/// `%APPDATA%/whispr-open/assets/`.
pub fn assets_dir() -> PathBuf {
    app_data_dir().join("assets")
}

/// Rutas de las que ya se avisó que faltaban: la ruta se comprueba en cada
/// vuelta del dibujo (~60 Hz), así que el aviso sale UNA vez por ruta y no
/// llena el log.
static WARNED_MISSING: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Apunta «falta el fichero, se pinta el tema» una sola vez por `kind`+`path`.
/// Devuelve `true` si el aviso lo dio esta llamada (los tests lo usan para
/// comprobar que no se repite).
pub fn warn_missing_once(kind: &str, path: &str) -> bool {
    let key = format!("{kind}|{path}");
    let mut guard = WARNED_MISSING.lock().unwrap_or_else(|e| e.into_inner());
    let seen = guard.get_or_insert_with(HashSet::new);
    if !seen.insert(key) {
        return false;
    }
    drop(guard);
    crate::log!(
        "custom",
        "asset desaparecido, cayendo al tema: {kind} «{path}»"
    );
    true
}

/// Como [`resolve_static`] pero avisa (una vez por ruta) cuando la ruta del
/// config ya no sirve. Es lo que tienen que usar los getters de la interfaz:
/// así el cambio no pasa desapercibido.
pub fn resolve_static_or_warn(kind: &str, path: Option<&str>) -> Option<PathBuf> {
    let resolved = resolve_static(path);
    if resolved.is_none() {
        if let Some(p) = path.map(str::trim).filter(|s| !s.is_empty()) {
            warn_missing_once(kind, p);
        }
    }
    resolved
}

/// Como [`resolve_frames`] pero avisa (una vez por ruta) cuando la carpeta del
/// config ya no existe o se quedó sin imágenes.
pub fn resolve_frames_or_warn(kind: &str, path: Option<&str>) -> Option<Vec<PathBuf>> {
    let resolved = resolve_frames(path);
    if resolved.is_none() {
        if let Some(p) = path.map(str::trim).filter(|s| !s.is_empty()) {
            warn_missing_once(kind, p);
        }
    }
    resolved
}

/// Comprueba una ruta de imagen del config:
///
/// - Vacía o en blanco → `None` (apagada a propósito, sin aviso).
/// - Ruta que sirve → `Some(ruta original, sin tocar)`.
/// - Ruta que ya no existe o formato no aceptado → aviso en el log + `None`
///   (quien llama la borra del config: no se deja una ruta muerta).
pub fn sanitize_static_path(kind: &str, path: Option<&str>) -> Option<String> {
    let trimmed = path.map(str::trim).filter(|s| !s.is_empty())?;
    if is_loadable(Path::new(trimmed)) {
        return Some(trimmed.to_string());
    }
    warn_missing_once(kind, trimmed);
    None
}

/// Lo mismo que [`sanitize_static_path`] con la carpeta de imágenes: se queda
/// solo si la carpeta existe y tiene al menos una imagen aceptada.
pub fn sanitize_frames_path(kind: &str, path: Option<&str>) -> Option<String> {
    let trimmed = path.map(str::trim).filter(|s| !s.is_empty())?;
    if resolve_frames(Some(trimmed)).is_some() {
        return Some(trimmed.to_string());
    }
    warn_missing_once(kind, trimmed);
    None
}

/// Limpia las tres rutas de personalización de `config` (H4). Devuelve `true`
/// si cambió algo — quien llama decide si guardar el config.
///
/// No mueve las rutas que siguen funcionando (compatibilidad con configs
/// viejas): solo quita las que apuntan a ficheros o carpetas que no están,
/// después de decirlo en el log.
pub fn sanitize_config_assets(config: &mut crate::config::Config) -> bool {
    let mut changed = false;

    let bg = sanitize_static_path("imagen de fondo de la ventana", config.bg_image_path.as_deref());
    if bg != config.bg_image_path {
        config.bg_image_path = bg;
        changed = true;
    }
    let sprite = sanitize_static_path("sprite de la burbuja", config.bubble_image_path.as_deref());
    if sprite != config.bubble_image_path {
        config.bubble_image_path = sprite;
        changed = true;
    }
    let frames = sanitize_frames_path(
        "carpeta de frames de la burbuja",
        config.bubble_frames_path.as_deref(),
    );
    if frames != config.bubble_frames_path {
        config.bubble_frames_path = frames;
        changed = true;
    }

    if changed {
        crate::log!(
            "custom",
            "rutas de personalización inexistentes eliminadas del config (la app usa el tema)"
        );
    }
    changed
}

/// Nombre para la copia de un fichero: la parte bonita de la ruta más una
/// huella corta de la ruta de origen, para que dos ficheros con el mismo
/// nombre en carpetas distintas no se pisen.
fn asset_file_name(src: &Path) -> Option<String> {
    let stem = src.file_stem()?.to_string_lossy().into_owned();
    let ext = src.extension()?.to_string_lossy().to_ascii_lowercase();
    let stem = sanitize_component(&stem);
    let fingerprint = short_fingerprint(&[&src.to_string_lossy()]);
    Some(format!("{stem}-{fingerprint}.{ext}"))
}

/// Nombre para la copia de una secuencia de imágenes: nombre limpio más la
/// huella de la lista, para que dos juegos distintos de la misma carpeta no
/// se pisen.
fn frames_dir_name(src: &Path, frames: &[PathBuf]) -> Option<String> {
    let stem = sanitize_component(&src.file_name()?.to_string_lossy());
    let mut parts: Vec<String> = vec![src.to_string_lossy().into_owned()];
    parts.push(frames.len().to_string());
    for f in frames {
        if let Some(n) = f.file_name() {
            parts.push(n.to_string_lossy().into_owned());
        }
    }
    let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
    let fingerprint = short_fingerprint(&refs);
    Some(format!("{stem}-{fingerprint}"))
}

/// Trozo de ruta seguro (sin separadores ni caracteres de control): solo
/// letras, números, `-`, `_` y `.`; lo demás pasa a `_`. Si queda vacío → `asset`.
fn sanitize_component(raw: &str) -> String {
    let cleaned: String = raw
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .take(48)
        .collect();
    let cleaned = cleaned.trim_matches(['_', '.']).to_string();
    if cleaned.is_empty() {
        "asset".to_string()
    } else {
        cleaned
    }
}

/// Huella estable de 8 dígitos hexadecimales de los datos dados. `DefaultHasher`
/// no es criptográfico: aquí solo sirve para que no choquen nombres de copia.
fn short_fingerprint(parts: &[&str]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for p in parts {
        p.hash(&mut hasher);
        0xffu8.hash(&mut hasher);
    }
    format!("{:08x}", (hasher.finish() & 0xffff_ffff) as u32)
}

/// ¿`path` está dentro de la carpeta `dir`? Se comparan las dos limpias
/// (`canonicalize`): en Windows eso añade el prefijo `\\?\`, que rompería un
/// `starts_with` contra la ruta escrita a mano.
fn path_inside(dir: &Path, path: &Path) -> bool {
    let dir_abs = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    path.starts_with(&dir_abs)
}

/// Copia `src` (una imagen que sirve) a `base` y devuelve la copia.
/// `None` si `src` no es una imagen aceptada o si la copia falla.
///
/// Si `src` YA está dentro de `base`, se devuelve tal cual (elegir otra vez la
/// copia no crea copias dentro de copias).
pub fn import_asset_into(base: &Path, src: &Path) -> Option<PathBuf> {
    if !is_loadable(src) {
        return None;
    }
    let src_abs = std::fs::canonicalize(src).unwrap_or_else(|_| src.to_path_buf());
    if path_inside(base, &src_abs) {
        return Some(src_abs);
    }
    std::fs::create_dir_all(base).ok()?;
    let dest = base.join(asset_file_name(&src_abs)?);
    if !path_inside(&dest, &src_abs) {
        std::fs::copy(&src_abs, &dest).ok()?;
    }
    Some(dest)
}

/// Copia `src` a `%APPDATA%/whispr-open/assets/` y devuelve la copia (la ruta
/// que tiene que quedar en el config).
pub fn import_asset(src: &Path) -> Option<PathBuf> {
    import_asset_into(&assets_dir(), src)
}

/// Copia la carpeta de imágenes `src` a `<base>/frames/<nombre>-<huella>` y
/// devuelve la copia. `None` si `src` no es una carpeta con imágenes aceptadas
/// o si la copia falla.
pub fn import_frames_dir_into(base: &Path, src: &Path) -> Option<PathBuf> {
    let frames = resolve_frames(Some(&src.to_string_lossy()))?;
    let src_abs = std::fs::canonicalize(src).unwrap_or_else(|_| src.to_path_buf());
    if path_inside(base, &src_abs) {
        return Some(src_abs);
    }
    std::fs::create_dir_all(base).ok()?;
    let dest = base.join("frames").join(frames_dir_name(&src_abs, &frames)?);
    if !path_inside(&dest, &src_abs) {
        std::fs::create_dir_all(&dest).ok()?;
        for frame in &frames {
            let name = frame.file_name()?;
            std::fs::copy(frame, dest.join(name)).ok()?;
        }
    }
    Some(dest)
}

/// Copia la carpeta de imágenes `src` a `%APPDATA%/whispr-open/assets/frames/`
/// y devuelve la copia (la ruta que tiene que quedar en el config).
pub fn import_frames_dir(src: &Path) -> Option<PathBuf> {
    import_frames_dir_into(&assets_dir(), src)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Crea un fichero temporal con una extensión concreta y devuelve su ruta.
    /// `None` si no se pudo crear la carpeta temporal. Se borra al acabar el
    /// test (Drop de `TempPath`).
    struct TempPath(PathBuf);

    impl Drop for TempPath {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn make_temp_file(name: &str, ext: &str) -> Option<TempPath> {
        let mut dir = std::env::temp_dir();
        dir.push(format!("whispr_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).ok()?;
        let mut p = dir;
        p.push(format!("{name}.{ext}"));
        let mut f = std::fs::File::create(&p).ok()?;
        // 1 byte de "imagen": para `is_supported_extension` basta la
        // extensión; iced no comprueba nada hasta dibujar, y el caso sin
        // fichero está cubierto aparte.
        f.write_all(b"\x89PNG\r\n\x1a\n").ok()?;
        Some(TempPath(p))
    }

    #[test]
    fn gif_de_un_solo_archivo_se_detecta_por_extension() {
        assert!(is_gif_path(Path::new("burbuja.GIF")));
        assert!(is_gif_path(Path::new("burbuja.gif")));
        assert!(!is_gif_path(Path::new("burbuja.png")));
        assert!(!is_gif_path(Path::new("burbuja")));
    }

    #[test]
    fn gif_inexistente_o_no_gif_no_decodifica() {
        assert!(decode_gif_frames(Path::new(r"C:\no-existe\x.gif")).is_none());
        assert!(decode_gif_frames(Path::new("foto.png")).is_none());
    }

    #[test]
    fn extension_supported_tolera_case_y_ext_comunes() {
        assert!(is_supported_extension(Path::new("foto.PNG")));
        assert!(is_supported_extension(Path::new("foto.jpg")));
        assert!(is_supported_extension(Path::new("foto.JPEG")));
        assert!(is_supported_extension(Path::new("f.bmp")));
        assert!(is_supported_extension(Path::new("f.gif")));
        assert!(is_supported_extension(Path::new("f.webp")));
        assert!(is_supported_extension(Path::new("f.tiff")));
        assert!(!is_supported_extension(Path::new("foto.txt")));
        assert!(!is_supported_extension(Path::new("foto")));
        // vacía / sin extensión
        assert!(!is_supported_extension(Path::new("")));
    }

    #[test]
    fn resolve_static_none_y_vacia_y_blanco_son_none() {
        // Lo que pide el producto: ruta vacía = sin imagen = aspecto de siempre.
        assert_eq!(resolve_static(None), None);
        assert_eq!(resolve_static(Some("")), None);
        assert_eq!(resolve_static(Some("   ")), None);
    }

    #[test]
    fn resolve_static_inexistente_es_none() {
        // Ruta que no existe (aunque la extensión sea buena): aspecto de siempre.
        assert_eq!(
            resolve_static(Some("C:\\nope\\missing.png")),
            None
        );
    }

    #[test]
    fn resolve_static_archivo_valido_se_devuelve() {
        let tmp = make_temp_file("bg", "png").expect("crear temp");
        let ruta = tmp.0.to_string_lossy().into_owned();
        let res = resolve_static(Some(&ruta));
        assert!(res.is_some(), "ruta válida debería resolver");
        assert!(res.unwrap().ends_with("bg.png"));
    }

    #[test]
    fn resolve_static_extension_invalida_es_none() {
        let tmp = make_temp_file("doc", "txt").expect("crear temp");
        let ruta = tmp.0.to_string_lossy().into_owned();
        assert_eq!(
            resolve_static(Some(&ruta)),
            None,
            "extensión no soportada → fallback"
        );
    }

    #[test]
    fn resolve_frames_none_y_no_directorio() {
        assert_eq!(resolve_frames(None), None);
        assert_eq!(resolve_frames(Some("")), None);
        assert_eq!(resolve_frames(Some("   ")), None);
        assert_eq!(
            resolve_frames(Some("C:\\nope\\missing_dir")),
            None,
            "directorio inexistente → fallback"
        );
    }

    #[test]
    fn resolve_frames_vacio_o_sin_extensiones() {
        // Carpeta vacía.
        let dir = std::env::temp_dir().join(format!(
            "whispr_test_empty_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let res = resolve_frames(Some(&dir.to_string_lossy()));
        assert_eq!(res, None);
        std::fs::remove_dir(&dir).ok();

        // Carpeta con un .txt → no cuenta como imagen.
        let dir = std::env::temp_dir().join(format!(
            "whispr_test_notext_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("not_image.txt"), b"hola").unwrap();
        let res = resolve_frames(Some(&dir.to_string_lossy()));
        assert_eq!(res, None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_frames_devuelve_orden_alfabetico() {
        let dir = std::env::temp_dir().join(format!(
            "whispr_test_frames_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["c.png", "a.png", "b.png", "z.jpg", "x.txt"] {
            std::fs::write(dir.join(name), b"\x89PNG\r\n\x1a\n").unwrap();
        }
        let res = resolve_frames(Some(&dir.to_string_lossy()))
            .expect("al menos un frame");
        // Los .txt se descartan; el resto, ordenado.
        assert_eq!(res.len(), 4);
        let nombres: Vec<String> = res
            .iter()
            .map(|p| {
                p.file_name().unwrap().to_string_lossy().into_owned()
            })
            .collect();
        assert_eq!(nombres, vec!["a.png", "b.png", "c.png", "z.jpg"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn next_frame_index_envuelve_y_normaliza() {
        // Vuelta básica.
        assert_eq!(next_frame_index(0, 3), 1);
        assert_eq!(next_frame_index(1, 3), 2);
        assert_eq!(next_frame_index(2, 3), 0, "último → primero");
        // frames_len=0 → 0 (caso raro).
        assert_eq!(next_frame_index(99, 0), 0);
        // `current` fuera de rango (más allá del último) se lleva al último
        // antes de avanzar: 100 con 3 imágenes cuenta como 2.
        assert_eq!(
            next_frame_index(100, 3),
            0,
            "100 con 3 frames normaliza a 2 y avanza a 0"
        );
        // 1 imagen: se queda en 0.
        assert_eq!(next_frame_index(0, 1), 0);
        // `saturating_add`: `usize::MAX` no puede dar pánico.
        assert_eq!(next_frame_index(usize::MAX, 2), 0);
    }

    #[test]
    fn clamp_fps_respeta_rango_y_deafult() {
        // Dentro del rango: pasa tal cual.
        assert_eq!(clamp_fps(1), 1);
        assert_eq!(clamp_fps(12), 12);
        assert_eq!(clamp_fps(30), 30);
        // Fuera del rango: se usa el valor por defecto.
        assert_eq!(clamp_fps(0), DEFAULT_BUBBLE_FPS);
        assert_eq!(clamp_fps(31), DEFAULT_BUBBLE_FPS);
        assert_eq!(clamp_fps(255), DEFAULT_BUBBLE_FPS);
    }

    #[test]
    fn interval_from_fps_calcula_y_redondea() {
        // 12 fps → 83 ms (1000/12 ≈ 83,33).
        assert_eq!(
            interval_from_fps(12),
            std::time::Duration::from_millis(83)
        );
        // 30 fps → 33 ms (1000/30 ≈ 33,33).
        assert_eq!(
            interval_from_fps(30),
            std::time::Duration::from_millis(33)
        );
        // 1 fps → 1000 ms.
        assert_eq!(
            interval_from_fps(1),
            std::time::Duration::from_millis(1000)
        );
        // 0 → valor por defecto (12 fps → 83 ms). Cubre el caso «no se validó».
        assert_eq!(
            interval_from_fps(0),
            std::time::Duration::from_millis(83)
        );
    }

    #[test]
    fn defaults_consistentes() {
        // El valor por defecto tiene que estar dentro del rango bueno; si
        // alguien lo sube de 30, el test falla sin tocar el rango.
        assert!(DEFAULT_BUBBLE_FPS >= MIN_BUBBLE_FPS);
        assert!(DEFAULT_BUBBLE_FPS <= MAX_BUBBLE_FPS);
    }

    // ── H4: guardar los ficheros y avisar cuando desaparecen ────────────────

    /// Carpeta temporal propia de cada test (se borra al salir).
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> TempDir {
            let dir = std::env::temp_dir().join(format!(
                "whispr_asset_{tag}_{}_{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ));
            std::fs::create_dir_all(&dir).expect("crear temp dir");
            TempDir(dir)
        }

        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write_png(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, b"\x89PNG\r\n\x1a\n").unwrap();
    }

    #[test]
    fn assets_dir_cuelga_de_appdata() {
        let dir = assets_dir();
        assert!(dir.to_string_lossy().contains("whispr-open"));
        assert!(dir.to_string_lossy().contains("assets"));
        // La misma carpeta raíz que el log: una copia de %APPDATA%/whispr-open
        // se lleva config, log y assets.
        assert!(dir.starts_with(app_data_dir()));
        assert_eq!(app_data_dir(), crate::log::log_dir());
    }

    #[test]
    fn import_asset_copia_fuera_de_downloads_y_sanea_el_nombre() {
        let src_dir = TempDir::new("src");
        let base = TempDir::new("base");
        let src = src_dir.join("mi fondo (1).PNG");
        write_png(&src);

        let dest = import_asset_into(&base.0, &src).expect("copia");
        assert!(dest.starts_with(&base.0), "la copia vive en la base: {dest:?}");
        assert!(dest.is_file(), "la copia existe en disco");
        assert_eq!(
            std::fs::read(&dest).unwrap(),
            std::fs::read(&src).unwrap(),
            "contenido idéntico al original"
        );
        // El nombre pierde espacios, paréntesis y mayúsculas de la extensión.
        let name = dest.file_name().unwrap().to_string_lossy().into_owned();
        assert!(!name.contains(' '), "nombre saneado: {name}");
        assert!(!name.contains('('), "nombre saneado: {name}");
        assert!(name.ends_with(".png"), "extensión normalizada: {name}");

        // Elegir otra vez el mismo fichero no crea una copia nueva (mismo destino).
        let again = import_asset_into(&base.0, &src).expect("copia repetida");
        assert_eq!(dest, again, "el destino es estable");
        let copias = std::fs::read_dir(&base.0)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .count();
        assert_eq!(copias, 1, "no se acumulan copias del mismo asset");
    }

    #[test]
    fn import_asset_de_la_propia_carpeta_no_anida_copias() {
        let base = TempDir::new("base");
        let ya_dentro = base.join("fondo-1234abcd.png");
        write_png(&ya_dentro);
        let devuelto = import_asset_into(&base.0, &ya_dentro).expect("ruta válida");
        let copias = std::fs::read_dir(&base.0)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .count();
        assert_eq!(copias, 1, "re-elegir la copia no crea otra copia");
        assert!(devuelto.is_file());
    }

    #[test]
    fn import_asset_rechaza_lo_que_no_es_imagen() {
        let src_dir = TempDir::new("src");
        let base = TempDir::new("base");
        let txt = src_dir.join("notas.txt");
        std::fs::write(&txt, b"hola").unwrap();
        assert_eq!(import_asset_into(&base.0, &txt), None);
        assert_eq!(import_asset_into(&base.0, &src_dir.join("nada.png")), None);
        assert!(!base.0.join("nada.png").exists());
    }

    #[test]
    fn import_frames_dir_copia_la_secuencia_completa() {
        let src_dir = TempDir::new("frames_src");
        let base = TempDir::new("base");
        let frames = src_dir.join("frames");
        for name in ["a.png", "b.png", "c.png"] {
            write_png(&frames.join(name));
        }
        // Un fichero que NO es imagen: no se copia como fotograma.
        std::fs::write(frames.join("leeme.txt"), b"txt").unwrap();

        let dest = import_frames_dir_into(&base.0, &frames).expect("copia");
        assert!(dest.starts_with(base.0.join("frames")), "vive en frames/: {dest:?}");
        // La copia es una carpeta de imágenes que el dibujo puede usar.
        let resueltos = resolve_frames(Some(&dest.to_string_lossy())).expect("frames copiados");
        assert_eq!(resueltos.len(), 3, "solo imágenes soportadas");
        // El .txt no se copia.
        assert!(!dest.join("leeme.txt").exists());
        // Elegir otra vez la copia no crea otra carpeta: devuelve la que ya hay.
        let again = import_frames_dir_into(&base.0, &dest).expect("ruta ya copiada");
        assert_eq!(
            std::fs::canonicalize(&dest).unwrap(),
            std::fs::canonicalize(&again).unwrap(),
            "re-elegir la copia devuelve la misma carpeta"
        );
        let subcarpetas = std::fs::read_dir(base.0.join("frames"))
            .unwrap()
            .filter_map(|e| e.ok())
            .count();
        assert_eq!(subcarpetas, 1, "no se anidan copias de la copia");
    }

    #[test]
    fn sanitize_static_path_limpia_lo_muerto_y_respeta_lo_vivo() {
        let dir = TempDir::new("sanitize");
        let vivo = dir.join("fondo.png");
        write_png(&vivo);
        let viva_s = vivo.to_string_lossy().into_owned();

        // Ruta que existe: se devuelve tal cual (no se cambia a la fuerza).
        assert_eq!(
            sanitize_static_path("fondo de ventana", Some(&viva_s)),
            Some(viva_s.clone())
        );
        // Vacía o en blanco: apagada a propósito, sin ruido.
        assert_eq!(sanitize_static_path("fondo de ventana", Some("   ")), None);
        assert_eq!(sanitize_static_path("fondo de ventana", None), None);
        // Ya no está: se borra.
        let muerta = dir.join("borrado.png").to_string_lossy().into_owned();
        assert_eq!(sanitize_static_path("fondo de ventana", Some(&muerta)), None);
        // Y con extensión no aceptada, también.
        let txt = dir.join("foto.txt").to_string_lossy().into_owned();
        assert_eq!(sanitize_static_path("fondo de ventana", Some(&txt)), None);
    }

    #[test]
    fn sanitize_frames_path_exige_directorio_con_imagenes() {
        let dir = TempDir::new("frames_sanitize");
        let con_frames = dir.join("frames");
        write_png(&con_frames.join("a.png"));
        let viva = con_frames.to_string_lossy().into_owned();
        assert_eq!(sanitize_frames_path("frames de la burbuja", Some(&viva)), Some(viva));

        let vacia = dir.join("vacia");
        std::fs::create_dir_all(&vacia).unwrap();
        let vacia_s = vacia.to_string_lossy().into_owned();
        assert_eq!(sanitize_frames_path("frames de la burbuja", Some(&vacia_s)), None);

        let inexistente = dir.join("no_existe").to_string_lossy().into_owned();
        assert_eq!(
            sanitize_frames_path("frames de la burbuja", Some(&inexistente)),
            None
        );
    }

    #[test]
    fn el_aviso_de_asset_desaparecido_solo_sale_una_vez_por_ruta() {
        // Clave distinta por test para no chocar con el estado compartido.
        let kind = "test-dedupe";
        let path = format!("C:\\nope\\desaparecido_{}.png", std::process::id());
        assert!(warn_missing_once(kind, &path), "el primer aviso se emite");
        assert!(
            !warn_missing_once(kind, &path),
            "el segundo ya no (el render valida ~60 veces/s)"
        );
        // `resolve_*_or_warn` no resuelve, pero avisa una sola vez.
        assert_eq!(resolve_static_or_warn(kind, Some(&path)), None);
        assert!(
            !warn_missing_once(kind, &path),
            "tras el getter sigue sin repetirse"
        );
    }

    #[test]
    fn sanitize_config_assets_quita_rutas_muertas_y_conserva_las_vivas() {
        let dir = TempDir::new("config_assets");
        let vivo = dir.join("fondo.png");
        write_png(&vivo);
        let viva_s = vivo.to_string_lossy().into_owned();
        let muerta = dir.join("borrado.png").to_string_lossy().into_owned();
        let frames_dir = dir.join("frames_viejos");
        std::fs::create_dir_all(&frames_dir).unwrap();
        let frames_s = frames_dir.to_string_lossy().into_owned();

        let mut config = crate::config::Config::default();
        config.bg_image_path = Some(viva_s.clone());
        config.bubble_image_path = Some(muerta.clone());
        config.bubble_frames_path = Some(frames_s.clone());

        let changed = sanitize_config_assets(&mut config);
        assert!(changed, "se limpiaron rutas muertas");
        assert_eq!(config.bg_image_path.as_deref(), Some(viva_s.as_str()));
        assert_eq!(config.bubble_image_path, None);
        assert_eq!(config.bubble_frames_path, None);

        // Un config que ya está bien no se toca (segunda pasada: igual).
        assert!(
            !sanitize_config_assets(&mut config),
            "sin cambios no hay nada que persistir"
        );
    }

    #[test]
    fn config_sin_assets_no_se_toca() {
        let mut config = crate::config::Config::default();
        assert!(
            !sanitize_config_assets(&mut config),
            "todo desactivado = nada que limpiar"
        );
        assert_eq!(config.bg_image_path, None);
    }
}
