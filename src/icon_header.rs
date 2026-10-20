//! Test del icono multirresolución (header ICO + contenido BMP DIB).
//!
//! Verifica end-to-end:
//! - El script `assets/build_icon.py` se ejecuta y produce un `.ico`.
//! - El header ICONDIR (reserved, type, count) cumple el formato.
//! - Cada entrada: ancho/alto en {16,24,32,48,64,128,256}, bpp=32,
//!   plano=1, offset sin solapes, tamaño coherente, formato BMP DIB
//!   (compatibilidad máxima con el cargador de recursos de Windows).
//! - Decodificación de la entrada mayor: píxel central violeta opaco,
//!   esquina transparente.
//!
//! Se ejecuta con ``cargo test --lib icon_header::tests``.

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::process::Command;

    const SIZES: &[u16] = &[16, 24, 32, 48, 64, 128, 256];
    const ACCENT: (u8, u8, u8) = (0x7C, 0x6C, 0xFF);
    /// Color de la onda del símbolo (misma tinta que el SVG de Figma).
    const WAVE: (u8, u8, u8) = (0xF7, 0xF6, 0xF3);

    /// Encuentra un Python ejecutable (mismo orden que build.rs).
    fn find_python() -> Option<&'static str> {
        for c in &["python", "py", "python3"] {
            if Command::new(c).arg("-V").output().is_ok() {
                return Some(c);
            }
        }
        None
    }

    fn run_generator(out_dir: &PathBuf) -> PathBuf {
        let py = find_python().expect("no hay Python en PATH (probados: python, py, python3)");
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join("build_icon.py");
        let status = Command::new(py)
            .arg(&script)
            .arg(out_dir)
            .status()
            .unwrap_or_else(|e| panic!("no se pudo lanzar {py}: {e}"));
        assert!(
            status.success(),
            "el generador terminó con código {status}"
        );
        let ico = out_dir.join("whispr-open.ico");
        assert!(ico.exists(), "no se generó {}", ico.display());
        ico
    }

    fn parse_ico(path: &PathBuf) -> (u16, Vec<(u32, u32, u16, u32, u32, Vec<u8>)>) {
        let data = std::fs::read(path).expect("leer .ico");
        assert!(data.len() >= 6, "ICO demasiado pequeño: {} B", data.len());
        let reserved = u16::from_le_bytes([data[0], data[1]]);
        let ftype = u16::from_le_bytes([data[2], data[3]]);
        let count = u16::from_le_bytes([data[4], data[5]]);
        assert_eq!(reserved, 0, "ICONDIR.reserved debe ser 0, vale {reserved}");
        assert_eq!(ftype, 1, "ICONDIR.type debe ser 1 (icono), vale {ftype}");
        assert!(
            count >= SIZES.len() as u16,
            "ICONDIR.count debe ser >= {}, vale {count}",
            SIZES.len()
        );
        assert!(
            6 + 16 * count as usize <= data.len(),
            "entradas truncadas"
        );

        let mut entries = Vec::with_capacity(count as usize);
        let mut seen_offsets = std::collections::HashSet::new();
        for i in 0..count as usize {
            let base = 6 + i * 16;
            let chunk = &data[base..base + 16];
            let w = chunk[0] as u32;
            let h = chunk[1] as u32;
            let _cc = chunk[2];
            let planes = u16::from_le_bytes([chunk[3], 0]);
            let bpp = u16::from_le_bytes([chunk[4], chunk[5]]);
            let _rt_hi = u16::from_le_bytes([chunk[6], chunk[7]]);
            let size = u32::from_le_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]);
            let offset = u32::from_le_bytes([chunk[12], chunk[13], chunk[14], chunk[15]]);
            let width = if w == 0 { 256 } else { w };
            let height = if h == 0 { 256 } else { h };
            assert!(
                seen_offsets.insert(offset),
                "entrada {i} offset {offset} duplicado"
            );
            assert!(
                (offset as usize) + (size as usize) <= data.len(),
                "entrada {i} offset+size excede el archivo"
            );
            assert_eq!(bpp, 32, "entrada {i} bpp debe ser 32, vale {bpp}");
            assert!(
                planes >= 1,
                "entrada {i} planes debe ser >= 1, vale {planes}"
            );
            let payload =
                data[offset as usize..(offset as usize + size as usize)].to_vec();
            if !(payload.starts_with(&[40, 0, 0, 0])) {
                panic!("entrada {i} no es BITMAPINFOHEADER (formato no BMP DIB)");
            }
            entries.push((width, height, bpp, size, offset, payload));
        }
        (count, entries)
    }

    fn decode_bmp_dib(width: u32, height: u32, payload: &[u8]) -> Vec<u8> {
        let w = width as usize;
        let h = height as usize;
        let bih_size = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]])
            as usize;
        assert_eq!(bih_size, 40, "BITMAPINFOHEADER.size != 40");
        let xor_start = bih_size;
        let stride = w * 4;
        let mut out = vec![0u8; w * h * 4];
        for y in 0..h {
            let src_row = &payload[xor_start + (h - 1 - y) * stride
                ..xor_start + (h - y) * stride];
            for x in 0..w {
                let b = src_row[x * 4];
                let g = src_row[x * 4 + 1];
                let r = src_row[x * 4 + 2];
                let a = src_row[x * 4 + 3];
                out[(y * w + x) * 4] = r;
                out[(y * w + x) * 4 + 1] = g;
                out[(y * w + x) * 4 + 2] = b;
                out[(y * w + x) * 4 + 3] = a;
            }
        }
        out
    }

    #[test]
    fn icon_header_and_payload_are_valid() {
        // 1. Generar el icono en un directorio temporal.
        let tmp = std::env::temp_dir().join(format!(
            "whispr-open-icon-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&tmp).expect("mkdir tmp");
        let ico_path = run_generator(&tmp);

        // 2. Parsear header.
        let (count, entries) = parse_ico(&ico_path);

        // 3. Validar conjunto de tallas exacto.
        let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for (w, h, _bpp, _size, _offset, _payload) in &entries {
            assert_eq!(w, h, "entrada no cuadrada");
            assert!(
                SIZES.iter().any(|&s| s as u32 == *w),
                "tamaño inesperado: {w}"
            );
            assert!(seen.insert(*w), "tamaño {w} duplicado en el ICO");
        }
        assert!(
            seen.len() >= SIZES.len(),
            "faltan tallas: {} vs {}",
            seen.len(),
            SIZES.len()
        );

        // 4. Pixel check en la entrada más grande (decodificación BMP DIB).
        let (bw, bh, _, _, _, bp) = entries
            .iter()
            .max_by_key(|(w, _, _, _, _, _)| *w)
            .map(|(w, h, b, s, o, p)| (*w, *h, *b, *s, *o, p.as_slice()))
            .expect("al menos una entrada");
        let size = bw as usize;
        let rgba = decode_bmp_dib(bw, bh, bp);
        assert_eq!(rgba.len(), size * size * 4);

        let cx = size / 2;
        let cy = size / 2;
        let center = (cy * size + cx) * 4;
        let (r, g, b, a) = (
            rgba[center],
            rgba[center + 1],
            rgba[center + 2],
            rgba[center + 3],
        );
        // El centro cae en la onda de la marca (blanco roto), no en la losa:
        // el símbolo es losa violeta + burbuja oscura + onda clara.
        assert_eq!(
            (r, g, b),
            WAVE,
            "centro {size}x{size} no es la onda: ({r},{g},{b})"
        );
        assert!(a >= 200, "centro {size}x{size} no opaco: alpha={a}");

        // Y por encima de la burbuja (10 % del alto) se ve la losa violeta.
        let top = ((size / 10) * size + cx) * 4;
        assert_eq!(
            (rgba[top], rgba[top + 1], rgba[top + 2]),
            ACCENT,
            "losa {size}x{size} no violeta arriba"
        );

        let corner = 0;
        let a_corner = rgba[corner + 3];
        assert!(
            a_corner <= 16,
            "esquina {size}x{size} no transparente: alpha={a_corner}"
        );

        // 5. Limpieza.
        let _ = std::fs::remove_dir_all(&tmp);

        eprintln!(
            "OK icon_header: {count} entradas, tallas {:?}",
            SIZES.iter().map(|s| *s as u32).collect::<Vec<_>>()
        );
    }
}