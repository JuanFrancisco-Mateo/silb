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

use std::path::{Path, PathBuf};

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
}
