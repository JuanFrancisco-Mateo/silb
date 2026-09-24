//! Imágenes de usuario y animación por frames (Personalización III).
//!
//! El usuario puede elegir:
//!
//! - Una **imagen de fondo** para la ventana principal.
//! - Un **sprite estático** O una **secuencia de frames PNG** para la
//!   burbuja flotante (animación por conmutación manual: iced 0.13 no
//!   expone un widget de GIF/WebP animado, requisito de la tarea;
//!   la migración a iced 0.14 — que ya lo trae — queda fuera de alcance).
//!
//! Reglas (verificado en tests de regresión):
//!
//! 1. **Rutas como dato, nunca hardcoded**. La ruta viene de config.json
//!    (`config.bg_image_path`, `config.bubble_image_path`,
//!    `config.bubble_frames_path`). Si el fichero no existe o el formato
//!    no es legible, el render vuelve al aspecto original (fallback
//!    obligatorio: nunca ventana en negro, nunca crash).
//! 2. **Animación sin gif nativo**: la lista de frames se ordena
//!    alfabéticamente (estable cross-platform en Windows) y se avanza
//!    con `iced::time::every` cada `1000/fps` ms. El frame index hace
//!    wrap (no desborda). Si la carpeta está vacía → fallback estático.
//! 3. **Desactivar todo es de primera clase**: rutas en blanco =
//!    apariencia original. El botón "Quitar" vacía la ruta
//!    correspondiente en config.json (sin perder la otra).
//!
//! ## Decisión sobre carga
//!
//! iced 0.13 `widget::image::Handle::from_path` exige una ruta que
//! exista en disco (la app intenta leer los bytes al renderizar, no
//! en la creación del Handle). Por eso la validación corre en este
//! módulo ANTES de construir el Handle: si el path no existe o no
//! es un archivo regular → `None` (el caller pinta el fallback). No
//! se cachea el Handle porque un reload rápido en caliente (al
//! re-elegir el mismo fichero) debe reflejarse inmediatamente.

use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Extensiones aceptadas para imágenes (en minúsculas, sin el punto).
/// Coincide con las capacidades del crate `image` 0.25 (PNG, JPEG,
/// BMP, GIF, WebP, TIFF, …). Lo que iced 0.13 puede decodificar
/// depende de las features del crate `image`; la feature `image`
/// de iced activa `image/default`, que incluye los codecs comunes.
const SUPPORTED_EXTS: &[&str] = &["png", "jpg", "jpeg", "bmp", "gif", "webp", "tiff"];

/// ¿La extensión del fichero está en `SUPPORTED_EXTS`?
/// Comparación en minúsculas; sin extensión → falso.
pub fn is_supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .map(|s| SUPPORTED_EXTS.iter().any(|e| *e == s))
        .unwrap_or(false)
}

/// ¿La ruta es utilizable como imagen de fondo / sprite?
///
/// Reglas (todas requeridas para que el widget iced no falle):
/// - El path existe y es un archivo regular (no un directorio).
/// - La extensión está en `SUPPORTED_EXTS`.
///
/// Esto NO decodifica el fichero (lo decodifica iced al dibujar);
/// el coste de un check adicional por frame es despreciable porque
/// solo se llama al cambiar la ruta en Ajustes o al construir el
/// `Loaded::current_frame` cada vez que se recarga.
pub fn is_loadable(path: &Path) -> bool {
    path.is_file() && is_supported_extension(path)
}

/// Resuelve la ruta almacenada en config:
///
/// - `None` o vacía / solo espacios → `None` (sin imagen).
/// - Trim + accesibilidad: si la ruta existe y tiene extensión
///   soportada, se devuelve normalizada (`canonicalize` cuando es
///   posible; cae a la ruta original si canonicalize falla por
///   permisos o por tratarse de una ruta recién escrita por el
///   diálogo de archivo antes de que el FS refresque).
///
/// Devolver `None` siempre significa "fallback al render original".
pub fn resolve_static(path: Option<&str>) -> Option<PathBuf> {
    let trimmed = path.map(str::trim).filter(|s| !s.is_empty())?;
    let p = PathBuf::from(trimmed);
    if !is_loadable(&p) {
        return None;
    }
    // canonicalize falla si la ruta no existe (en ese caso `is_loadable`
    // ya habría devuelto false), pero también si hay un problema de
    // permisos o si el path es muy largo en Windows. La ruta original
    // sigue siendo utilizable por iced: la canonicalización es solo
    // cosmética para el log.
    Some(std::fs::canonicalize(&p).unwrap_or(p))
}

/// Lista los frames de una carpeta, ordenados alfabéticamente.
///
/// - `None` o vacía → `None` (sin animación; el caller pinta sprite estático).
/// - Directorio inexistente → `None`.
/// - Directorio vacío o sin extensiones soportadas → `None`.
/// - Si hay al menos un frame válido → `Some(vec)` con las rutas en orden.
///
/// Se filtran: subdirectorios, ficheros sin extensión soportada, y
/// entradas que fallen el predicado `is_loadable` (por ejemplo,
/// enlaces simbólicos rotos). El orden es el de `read_dir` post-sort,
/// que es estable y case-insensitive en Windows (NTFS preserva el
/// orden de inserción del directorio; aquí ordenamos explícitamente
/// por `to_string_lossy` para que "frame_2.png" vaya detrás de
/// "frame_10.png" en lexicografía ASCII pura — se asume que los
/// nombres siguen el patrón del usuario).
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

/// Avanza el índice de frame N frames más (1 por convención del caller) y
/// devuelve el nuevo índice con wrap. Saturación segura: si `frames_len`
/// es 0 → 0; cualquier `current` fuera de rango se normaliza a 0 antes
/// de avanzar. Función pura (sin estado) para que los tests la cubran
/// sin un runtime de iced.
pub fn next_frame_index(current: usize, frames_len: usize) -> usize {
    if frames_len == 0 {
        return 0;
    }
    let cur = current.min(frames_len - 1);
    // saturating_add: si por algún motivo `current` es `usize::MAX`, no
    // desborda; sumamos 1 módulo frames_len.
    (cur + 1) % frames_len
}

/// FPS por defecto para la animación de la burbuja. Suficientemente
/// rápido para que un sprite se sienta "vivo" pero no consume CPU
/// (~16 ms/tick → ~60 Hz). El usuario puede bajarlo (1..=30).
pub const DEFAULT_BUBBLE_FPS: u8 = 12;

/// FPS tope aceptado. Por encima de esto el sprite se vuelve
/// inmanejable en pantallas estándar y el coste de CPU/IO del Poll
/// no compensa. 30 fps es el techo de sprites 2D clásicos (consola).
pub const MAX_BUBBLE_FPS: u8 = 30;

/// FPS mínimo aceptado. Por debajo de 1 la animación es
/// imperceptible; un valor de 0 rompería la división de
/// `interval_from_fps`. Se usa 1 como suelo.
pub const MIN_BUBBLE_FPS: u8 = 1;

/// Valida y normaliza un FPS elegido por el usuario. Fuera de rango
/// cae al DEFAULT. Función pura testeable.
pub fn clamp_fps(value: u8) -> u8 {
    if value < MIN_BUBBLE_FPS || value > MAX_BUBBLE_FPS {
        DEFAULT_BUBBLE_FPS
    } else {
        value
    }
}

/// Calcula el intervalo entre ticks a partir de un FPS validado.
/// Función pura; 0 frames por segundo cae al default (caso
/// patológico: el caller debería haber llamado a `clamp_fps` antes).
pub fn interval_from_fps(fps: u8) -> std::time::Duration {
    let fps = clamp_fps(fps).max(1);
    std::time::Duration::from_millis(1_000_u64 / u64::from(fps))
}

/// Mensaje del Poll que se traduce a `Message::AnimationTick` (en
/// `app.rs`) cuando hay animación activa. Existe como `pub` para que
/// el módulo que arma la subscription (`app::WhisprApp::subscription`)
/// pueda armar el `map` con esta misma firma.
pub fn tick_to_instant(_instant: std::time::Instant) -> std::time::Instant {
    // Wrapper trivial; el Poll real vive en `app.rs` y solo necesita
    // poder hacer `.map(Message::AnimationTick)` sobre la subscription
    // de tiempo. Mantener la indirección aquí permite evolucionar la
    // lógica del tick (p. ej. detectar stalls) sin tocar el message
    // dispatch de la UI.
    std::time::Instant::now()
}

/// ── Persistencia de assets (H4 del QA 24/09/2026) ─────────────────────────
///
/// El usuario elegía el fondo o los frames desde `Descargas` y el config
/// guardaba ESA ruta: al vaciar Descargas, la personalización desaparecía sin
/// avisar. Ahora `import_asset` / `import_frames_dir` copian el material a
/// `%APPDATA%/whispr-open/assets/` y el config guarda la copia, así que sobrevive
/// a limpiezas de carpetas temporales.
///
/// Compatibilidad: una config vieja con una ruta externa que SIGUE existiendo
/// se respeta tal cual (no se migra a la fuerza); solo se limpia del config
/// cuando apunta a algo que ya no está (`sanitize_config_assets`).

/// Carpeta de datos de la app (`%APPDATA%/whispr-open/`): la misma que usa el
/// log (`crate::log::log_dir`), para que un backup de esa carpeta lo incluya todo.
pub fn app_data_dir() -> PathBuf {
    crate::log::log_dir()
}

/// Carpeta donde la app guarda copia de los assets elegidos por el usuario:
/// `%APPDATA%/whispr-open/assets/`.
pub fn assets_dir() -> PathBuf {
    app_data_dir().join("assets")
}

/// Rutas ya avisadas como desaparecidas: el render valida la ruta en cada
/// frame (~60 Hz), así que el aviso se emite UNA vez por ruta y no inunda el log.
static WARNED_MISSING: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Registra «asset desaparecido, cayendo al tema» una sola vez por
/// `kind`+`path`. Devuelve `true` si esta llamada fue la que avisó (los tests
/// lo usan para comprobar la deduplicación).
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

/// Como [`resolve_static`] pero avisa (una vez por ruta) cuando la ruta
/// configurada ya no sirve. Es lo que deben usar los getters de la UI: el
/// fallback deja de ser silencioso.
pub fn resolve_static_or_warn(kind: &str, path: Option<&str>) -> Option<PathBuf> {
    let resolved = resolve_static(path);
    if resolved.is_none() {
        if let Some(p) = path.map(str::trim).filter(|s| !s.is_empty()) {
            warn_missing_once(kind, p);
        }
    }
    resolved
}

/// Como [`resolve_frames`] pero avisa (una vez por ruta) cuando la carpeta
/// configurada ya no existe o se quedó sin imágenes.
pub fn resolve_frames_or_warn(kind: &str, path: Option<&str>) -> Option<Vec<PathBuf>> {
    let resolved = resolve_frames(path);
    if resolved.is_none() {
        if let Some(p) = path.map(str::trim).filter(|s| !s.is_empty()) {
            warn_missing_once(kind, p);
        }
    }
    resolved
}

/// Comprueba una ruta de imagen configurada:
///
/// - Ruta vacía o en blanco → `None` (desactivado a propósito, sin aviso).
/// - Ruta utilizable → `Some(ruta original, sin tocar)`.
/// - Ruta que ya no existe / formato no soportado → aviso al log + `None`
///   (el caller la borra del config: no se deja una ruta muerta).
pub fn sanitize_static_path(kind: &str, path: Option<&str>) -> Option<String> {
    let trimmed = path.map(str::trim).filter(|s| !s.is_empty())?;
    if is_loadable(Path::new(trimmed)) {
        return Some(trimmed.to_string());
    }
    warn_missing_once(kind, trimmed);
    None
}

/// Igual que [`sanitize_static_path`] para la carpeta de frames: se mantiene
/// solo si el directorio existe y contiene al menos una imagen soportada.
pub fn sanitize_frames_path(kind: &str, path: Option<&str>) -> Option<String> {
    let trimmed = path.map(str::trim).filter(|s| !s.is_empty())?;
    if resolve_frames(Some(trimmed)).is_some() {
        return Some(trimmed.to_string());
    }
    warn_missing_once(kind, trimmed);
    None
}

/// Sanea las tres rutas de personalización de `config` (H4). Devuelve `true`
/// si algo cambió — el caller decide persistir el config.
///
/// No migra rutas que siguen funcionando (compatibilidad con configs viejas):
/// solo quita las que apuntan a ficheros/carpetas inexistentes, después de
/// decirlo en el log.
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

/// Nombre de fichero para la copia de un asset: componente saneado + huella
/// corta de la ruta de origen, para que dos ficheros homónimos de carpetas
/// distintas no se pisen.
fn asset_file_name(src: &Path) -> Option<String> {
    let stem = src.file_stem()?.to_string_lossy().into_owned();
    let ext = src.extension()?.to_string_lossy().to_ascii_lowercase();
    let stem = sanitize_component(&stem);
    let fingerprint = short_fingerprint(&[&src.to_string_lossy()]);
    Some(format!("{stem}-{fingerprint}.{ext}"))
}

/// Nombre de carpeta para la copia de una secuencia de frames: nombre saneado
/// + huella de la lista de frames (contar distintos juegos de frames de una
/// misma carpeta no debe pisarse).
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

/// Componente de ruta seguro (sin separadores ni caracteres de control): solo
/// alfanuméricos, `-`, `_` y `.`; el resto pasa a `_`. Vacío → `asset`.
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
/// no es criptográfico: aquí solo sirve para no colisionar nombres de copia.
fn short_fingerprint(parts: &[&str]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for p in parts {
        p.hash(&mut hasher);
        0xffu8.hash(&mut hasher);
    }
    format!("{:08x}", (hasher.finish() & 0xffff_ffff) as u32)
}

/// ¿`path` vive dentro del directorio `dir`? Ambos se comparan
/// canonicalizados: en Windows `canonicalize` devuelve el prefijo `\\?\`, que
/// rompería un `starts_with` contra la ruta escrita a mano.
fn path_inside(dir: &Path, path: &Path) -> bool {
    let dir_abs = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    path.starts_with(&dir_abs)
}

/// Copia `src` (una imagen utilizable) a `base` y devuelve la copia.
/// `None` si `src` no es una imagen soportada o si la copia falla.
///
/// Si `src` YA vive dentro de `base` se devuelve tal cual (re-elegir la copia
/// no genera copias anidadas).
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
/// que debe quedar en config).
pub fn import_asset(src: &Path) -> Option<PathBuf> {
    import_asset_into(&assets_dir(), src)
}

/// Copia la carpeta de frames `src` a `<base>/frames/<nombre>-<huella>` y
/// devuelve la copia. `None` si `src` no es una carpeta con imágenes
/// soportadas o si la copia falla.
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

/// Copia la carpeta de frames `src` a `%APPDATA%/whispr-open/assets/frames/`
/// y devuelve la copia (la ruta que debe quedar en config).
pub fn import_frames_dir(src: &Path) -> Option<PathBuf> {
    import_frames_dir_into(&assets_dir(), src)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Crea un fichero temporal con una extensión concreta y devuelve
    /// su ruta. `None` si el directorio temporal no se pudo crear.
    /// Se borra al final del test (Drop sobre `TempPath`).
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
        // 1 byte de "imagen": para el contrato de is_supported_extension
        // basta con la extensión; iced no valida hasta que dibuja, y la
        // ruta de fallback está cubierta por separado.
        f.write_all(b"\x89PNG\r\n\x1a\n").ok()?;
        Some(TempPath(p))
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
        // Requisito del producto: rutas vacías = sin imagen = fallback.
        assert_eq!(resolve_static(None), None);
        assert_eq!(resolve_static(Some("")), None);
        assert_eq!(resolve_static(Some("   ")), None);
    }

    #[test]
    fn resolve_static_inexistente_es_none() {
        // Ruta que no existe (aunque tenga extensión soportada): fallback.
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
        // Directorio vacío.
        let dir = std::env::temp_dir().join(format!(
            "whispr_test_empty_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let res = resolve_frames(Some(&dir.to_string_lossy()));
        assert_eq!(res, None);
        std::fs::remove_dir(&dir).ok();

        // Directorio con un .txt → no se cuenta como frame.
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
        // Los .txt se filtran; el resto, ordenados.
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
        // Wrap básico.
        assert_eq!(next_frame_index(0, 3), 1);
        assert_eq!(next_frame_index(1, 3), 2);
        assert_eq!(next_frame_index(2, 3), 0, "último → primero");
        // frames_len=0 → 0 (caso degenerado).
        assert_eq!(next_frame_index(99, 0), 0);
        // current fuera de rango (más allá del último) se normaliza al
        // último antes de avanzar: 100 con 3 frames se trata como 2.
        assert_eq!(
            next_frame_index(100, 3),
            0,
            "100 con 3 frames normaliza a 2 y avanza a 0"
        );
        // 1 frame: oscila entre 0 y 0.
        assert_eq!(next_frame_index(0, 1), 0);
        // saturating_add: usize::MAX no debe hacer panic.
        assert_eq!(next_frame_index(usize::MAX, 2), 0);
    }

    #[test]
    fn clamp_fps_respeta_rango_y_deafult() {
        // Dentro del rango: pasa tal cual.
        assert_eq!(clamp_fps(1), 1);
        assert_eq!(clamp_fps(12), 12);
        assert_eq!(clamp_fps(30), 30);
        // Fuera del rango: cae al default.
        assert_eq!(clamp_fps(0), DEFAULT_BUBBLE_FPS);
        assert_eq!(clamp_fps(31), DEFAULT_BUBBLE_FPS);
        assert_eq!(clamp_fps(255), DEFAULT_BUBBLE_FPS);
    }

    #[test]
    fn interval_from_fps_calcula_y_redondea() {
        // 12 fps → 83 ms (1000/12 ≈ 83.33).
        assert_eq!(
            interval_from_fps(12),
            std::time::Duration::from_millis(83)
        );
        // 30 fps → 33 ms (1000/30 ≈ 33.33).
        assert_eq!(
            interval_from_fps(30),
            std::time::Duration::from_millis(33)
        );
        // 1 fps → 1000 ms.
        assert_eq!(
            interval_from_fps(1),
            std::time::Duration::from_millis(1000)
        );
        // 0 → default (12 fps → 83 ms). Cubre el camino de "no se validó".
        assert_eq!(
            interval_from_fps(0),
            std::time::Duration::from_millis(83)
        );
    }

    #[test]
    fn defaults_consistentes() {
        // El default sensato debe estar dentro del rango válido; si
        // alguien lo sube de 30 el test cae sin necesidad de tocar el
        // rango.
        assert!(DEFAULT_BUBBLE_FPS >= MIN_BUBBLE_FPS);
        assert!(DEFAULT_BUBBLE_FPS <= MAX_BUBBLE_FPS);
    }

    // ── H4: persistencia de assets y aviso de asset desaparecido ──────────

    /// Directorio temporal propio de cada test (se borra al salir).
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
        // Misma carpeta raíz que el log: un backup de %APPDATA%/whispr-open
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
        // El nombre pierde espacios, paréntesis y mayúsculas de extensión.
        let name = dest.file_name().unwrap().to_string_lossy().into_owned();
        assert!(!name.contains(' '), "nombre saneado: {name}");
        assert!(!name.contains('('), "nombre saneado: {name}");
        assert!(name.ends_with(".png"), "extensión normalizada: {name}");

        // Re-elegir el mismo fichero no genera una copia nueva (mismo destino).
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
        // Un fichero que NO es imagen: no debe copiarse como frame.
        std::fs::write(frames.join("leeme.txt"), b"txt").unwrap();

        let dest = import_frames_dir_into(&base.0, &frames).expect("copia");
        assert!(dest.starts_with(base.0.join("frames")), "vive en frames/: {dest:?}");
        // La copia es una carpeta de frames utilizable por el render.
        let resueltos = resolve_frames(Some(&dest.to_string_lossy())).expect("frames copiados");
        assert_eq!(resueltos.len(), 3, "solo imágenes soportadas");
        // El .txt no se copia.
        assert!(!dest.join("leeme.txt").exists());
        // Re-elegir la copia no anida otra carpeta: devuelve la que ya existe.
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

        // Ruta que existe: se devuelve tal cual (sin migrar a la fuerza).
        assert_eq!(
            sanitize_static_path("fondo de ventana", Some(&viva_s)),
            Some(viva_s.clone())
        );
        // Vacía / en blanco: desactivado a propósito, sin ruido.
        assert_eq!(sanitize_static_path("fondo de ventana", Some("   ")), None);
        assert_eq!(sanitize_static_path("fondo de ventana", None), None);
        // Desaparecida: se limpia.
        let muerta = dir.join("borrado.png").to_string_lossy().into_owned();
        assert_eq!(sanitize_static_path("fondo de ventana", Some(&muerta)), None);
        // Y con extensión no soportada, también.
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
        // Clave única por test para no chocar con el estado estático global.
        let kind = "test-dedupe";
        let path = format!("C:\\nope\\desaparecido_{}.png", std::process::id());
        assert!(warn_missing_once(kind, &path), "el primer aviso se emite");
        assert!(
            !warn_missing_once(kind, &path),
            "el segundo ya no (el render valida ~60 veces/s)"
        );
        // Y `resolve_*_or_warn` no resuelve, pero avisa una sola vez.
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

        // Un config ya sano no se toca (segunda pasada: idempotente).
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
