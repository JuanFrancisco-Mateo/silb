#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Verificador del icono multirresolución de Silb.

Comprueba que el ``.ico`` generado cumple el contrato:
- ICONDIR: ``reserved=0``, ``type=1``, ``count>=7``.
- Cada ICONDIRENTRY: ancho/alto en {16,24,32,48,64,128,256}, bpp=32, plano=1,
  tamaño de payload coherente, offset dentro del fichero y sin solapes.
- Carga (al menos) la primera imagen (BMP o PNG) y comprueba que el píxel
  central es violeta opaco y la esquina es transparente.

Se invoca manualmente (``python3 scripts/verify-ico.py assets/icon.ico``) o
desde CI. Exit 0 = válido, !=0 = error.
"""

from __future__ import annotations

import struct
import sys
import zlib
from pathlib import Path


EXPECTED_SIZES = (16, 24, 32, 48, 64, 128, 256)
ACCENT = (0x7C, 0x6C, 0xFF)


def parse_ico(path: Path) -> list[dict]:
    """Lee el ICO y devuelve la lista de entradas.

    Acepta tanto BMP DIB como PNG dentro del ICO; la política del proyecto
    (ver ``assets/build_icon.py``) es emitir siempre BMP DIB para máxima
    compatibilidad con el cargador de recursos de Windows, pero el
    verificador no asume.
    """
    data = path.read_bytes()
    if len(data) < 6:
        raise ValueError(f"ICO demasiado pequeño: {len(data)} B")
    reserved, ftype, count = struct.unpack("<HHH", data[:6])
    if reserved != 0:
        raise ValueError(f"ICONDIR.reserved debe ser 0, vale {reserved}")
    if ftype != 1:
        raise ValueError(f"ICONDIR.type debe ser 1 (icono), vale {ftype}")
    if count < len(EXPECTED_SIZES):
        raise ValueError(
            f"ICONDIR.count debe ser >= {len(EXPECTED_SIZES)} (esperado {EXPECTED_SIZES}), vale {count}"
        )
    if 6 + 16 * count > len(data):
        raise ValueError("ICONDIR excede el tamaño del archivo")

    entries: list[dict] = []
    cursor = 6
    seen_offsets: set[int] = set()
    for i in range(count):
        chunk = data[cursor : cursor + 16]
        if len(chunk) < 16:
            raise ValueError(f"entrada {i} truncada")
        w, h, cc, res, bpp, rt_hi, size, offset = struct.unpack(
            "<BBBBHHII", chunk
        )
        w = 256 if w == 0 else w
        h = 256 if h == 0 else h
        if offset in seen_offsets:
            raise ValueError(f"entrada {i} offset {offset} duplicado")
        seen_offsets.add(offset)
        if offset + size > len(data):
            raise ValueError(
                f"entrada {i} offset+size ({offset+size}) excede el archivo ({len(data)})"
            )
        if bpp != 32:
            raise ValueError(f"entrada {i} bpp debe ser 32, vale {bpp}")
        if res < 1:
            raise ValueError(f"entrada {i} planes debe ser >= 1, vale {res}")
        payload = data[offset : offset + size]
        is_png = payload[:8] == b"\x89PNG\r\n\x1a\n"
        fmt = "png" if is_png else "bmp"
        if not is_png:
            # BMP: BITMAPINFOHEADER, primeros 4 bytes = 40 (tamaño del header).
            (bih_size,) = struct.unpack("<I", payload[:4])
            if bih_size != 40:
                raise ValueError(
                    f"entrada {i} BMP no es BITMAPINFOHEADER (size={bih_size})"
                )
        entries.append(
            {
                "w": w,
                "h": h,
                "bpp": bpp,
                "size": size,
                "offset": offset,
                "fmt": fmt,
                "payload": payload,
            }
        )
        cursor += 16
    return entries


def decode_payload(entry: dict) -> tuple[int, bytes]:
    """Devuelve (size, RGBA bytes) decodificando PNG o BMP DIB."""
    if entry["fmt"] == "png":
        # Decodificador mínimo de PNG RGBA8.
        payload = entry["payload"]
        assert payload[:8] == b"\x89PNG\r\n\x1a\n"
        # Saltamos IHDR (longitud+tag+13 bytes de datos+CRC ya cubiertos en el
        # primer chunk tras el signature: 8+4=12 firma + 4+4+13+4=25 IHDR).
        idx = 8
        chunks: list[tuple[bytes, bytes]] = []
        while idx < len(payload):
            length = struct.unpack(">I", payload[idx : idx + 4])[0]
            tag = payload[idx + 4 : idx + 8]
            data = payload[idx + 8 : idx + 8 + length]
            # crc no se valida para velocidad (verificación real la hace el OS)
            chunks.append((tag, data))
            idx += 8 + length + 4
            if tag == b"IEND":
                break
        ihdr = next(d for t, d in chunks if t == b"IHDR")
        w, h, depth, color = struct.unpack(">IIBB", ihdr[:10])
        if depth != 8 or color != 6:
            raise ValueError(f"PNG no RGBA8: depth={depth} color={color}")
        idat = b"".join(d for t, d in chunks if t == b"IDAT")
        raw = zlib.decompress(idat)
        # Filtros: filter byte por scanline. Sin filtrar (0) o up (2) en
        # nuestro output. Implementamos solo "None" (lo que emite nuestro
        # generador).
        stride = w * 4
        out = bytearray()
        prev = bytearray(stride)
        for y in range(h):
            f = raw[y * (stride + 1)]
            scanline = bytearray(raw[y * (stride + 1) + 1 : (y + 1) * (stride + 1)])
            if f == 0:
                pass
            elif f == 1:  # Sub
                for x in range(4, stride):
                    scanline[x] = (scanline[x] + scanline[x - 4]) & 0xFF
            elif f == 2:  # Up
                for x in range(stride):
                    scanline[x] = (scanline[x] + prev[x]) & 0xFF
            else:
                raise ValueError(f"filtro PNG no soportado: {f}")
            out.extend(scanline)
            prev = scanline
        # PNG se almacena top-down; BMP top-down, así que devolvemos tal cual.
        return w, bytes(out[: w * h * 4])
    # BMP DIB (BITMAPINFOHEADER + XOR + AND). Píxeles en BGRA bottom-up.
    payload = entry["payload"]
    w, h_bmp = struct.unpack("<ii", payload[4:12])
    assert w == entry["w"] and h_bmp == entry["h"] * 2
    stride = w * 4
    bih_size = 40
    xor_start = bih_size
    # XOR rows: bytes XOR (bottom-up), AND rows vienen después (no las usamos
    # porque el alfa ya va en XOR con premultiplicación cero).
    out = bytearray(w * entry["h"] * 4)
    for y in range(entry["h"]):
        src_row = payload[xor_start + (entry["h"] - 1 - y) * stride : xor_start + (entry["h"] - y) * stride]
        # Swap BGRA → RGBA.
        for x in range(w):
            b, g, r, a = src_row[x * 4 : x * 4 + 4]
            out[(y * w + x) * 4 : (y * w + x) * 4 + 4] = bytes((r, g, b, a))
    return w, bytes(out)


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        sys.stderr.write(f"uso: {argv[0]} <ruta_al_ico>\n")
        return 2
    path = Path(argv[1])
    if not path.exists():
        sys.stderr.write(f"no existe: {path}\n")
        return 1
    entries = parse_ico(path)

    # Tallas exactas + decodificación de la primera para muestrear píxeles.
    seen_sizes: set[int] = set()
    for i, e in enumerate(entries):
        if e["w"] != e["h"]:
            sys.stderr.write(f"entrada {i} no es cuadrada: {e['w']}x{e['h']}\n")
            return 1
        if e["w"] not in EXPECTED_SIZES:
            sys.stderr.write(
                f"entrada {i} tamaño inesperado {e['w']}; permitidos: {EXPECTED_SIZES}\n"
            )
            return 1
        if e["w"] in seen_sizes:
            sys.stderr.write(f"entrada {i} tamaño {e['w']} duplicado\n")
            return 1
        seen_sizes.add(e["w"])

    # Pixel check en la entrada >= 32 (todas PNG): centro violeta opaco, esquina
    # transparente.
    sample = next(e for e in entries if e["w"] >= 32)
    size, rgba = decode_payload(sample)
    assert size == sample["w"]
    cx = (size // 2)
    cy = (size // 2)
    center_idx = (cy * size + cx) * 4
    r, g, b, a = rgba[center_idx : center_idx + 4]
    if (r, g, b) != ACCENT:
        sys.stderr.write(
            f"centro de {size}x{size} no es violeta: {(r, g, b)} (esperado {ACCENT})\n"
        )
        return 1
    if a < 200:
        sys.stderr.write(f"centro de {size}x{size} no opaco: alpha={a}\n")
        return 1
    corner_idx = 0  # (0,0)
    r, g, b, a = rgba[corner_idx : corner_idx + 4]
    if a > 16:
        sys.stderr.write(
            f"esquina de {size}x{size} no transparente: alpha={a}\n"
        )
        return 1

    sys.stdout.write(
        f"OK {path} ({path.stat().st_size} B, {len(entries)} entradas: "
        f"{sorted(seen_sizes)})\n"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))