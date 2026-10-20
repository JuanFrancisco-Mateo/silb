#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Generador determinista del icono multirresolución de Silb.

Salida: ``<out>/whispr-open.ico`` con entradas a 16, 24, 32, 48, 64, 128 y 256
px, todas con el mismo lenguaje visual (círculo violeta #7C6CFF sobre fondo
transparente) que el icono de la bandeja generado en ``src/tray.rs``.

Por qué Python y por qué sin dependencias externas:
- Determinista: misma semilla/colores/tamaños en cada build, sin red.
- Sin PIL/Pillow: el script escribe PNGs a mano (formato trivial, deflate
  puro de la stdlib). Las imágenes van dentro del contenedor ICO en formato
  PNG (ICO admite PNG para tamaños >= 32 px; para 16/24 px usa BMP DIB
  porque algunos visores antiguos no soportan PNG a esas resoluciones,
  aunque Windows 10/11 sí).
- Se invoca desde ``build.rs`` en cada compilación, así el icono y el binario
  siempre están sincronizados.

Uso CLI:
    python3 assets/build_icon.py <directorio_salida>

Exit code 0 = icono escrito y verificado por tamaño de archivo.
Exit code != 0 = error (mensaje por stderr).
"""

from __future__ import annotations

import os
import struct
import sys
import zlib
from pathlib import Path


# Tallas requeridas por docs/EMPAQUETADO.md §3 y por Windows (256 es
# obligatorio para Windows 11; 16/24/32/48/64/128 cubren shell, ALT+TAB y
# menús contextuales de la barra de tareas).
SIZES: tuple[int, ...] = (16, 24, 32, 48, 64, 128, 256)

# Color del acento de la app (theme::ACCENT) — círculo violeta opaco sobre
# fondo transparente. El borde se suaviza con 1 px de transición lineal
# (mismo algoritmo que src/tray.rs::circle_rgba, replicado aquí para que el
# script sea independiente de la fuente de la app).
ACCENT_RGB: tuple[int, int, int] = (0x7C, 0x6C, 0xFF)


def _mark_sample(nx: float, ny: float) -> tuple[bool, bool]:
    """Punto normalizado (0..1) → (dentro del cuadrado, dentro de la onda).

    La marca es un cuadrado violeta de esquinas redondeadas con una onda de
    audio blanca de 5 barras. Se evalúa por muestras para poder antialiasear
    con supersampling sin dependencias.
    """
    # Cuadrado redondeado: recorte por esquinas (radio 22 % del lado).
    r = 0.22
    cx = min(max(nx, r), 1.0 - r)
    cy = min(max(ny, r), 1.0 - r)
    if (nx - cx) ** 2 + (ny - cy) ** 2 >= r * r:
        return (False, False)
    # Onda: 5 barras verticales centradas, alturas fijas (silueta reconocible
    # a 16 px). Zona útil del 20 % al 80 % del lado.
    heights = (0.34, 0.62, 1.0, 0.62, 0.34)
    bar_w = 0.088
    gap = 0.021
    total = len(heights) * bar_w + (len(heights) - 1) * gap
    x0 = 0.5 - total / 2.0
    for i, h in enumerate(heights):
        bx0 = x0 + i * (bar_w + gap)
        bx1 = bx0 + bar_w
        if bx0 <= nx < bx1:
            half = h * 0.30
            return (True, abs(ny - 0.5) < half)
    return (True, False)


def mark_rgba(size: int) -> bytes:
    """Buffer RGBA ``size * size`` con la marca de Silb (cuadrado + onda)."""
    assert size > 0
    buf = bytearray()
    # 3x3 de supersampling: suficiente para que a 16 px no salgan bordes
    # dentados y barato (256²·9 muestras tarda <1 s).
    offs = (0.1667, 0.5, 0.8333)
    inv = 1.0 / 9.0
    for y in range(size):
        for x in range(size):
            in_mark = 0
            in_wave = 0
            for oy in offs:
                for ox in offs:
                    m, w = _mark_sample((x + ox) / size, (y + oy) / size)
                    if m:
                        in_mark += 1
                        if w:
                            in_wave += 1
            if in_mark == 0:
                buf += b"\x00\x00\x00\x00"
                continue
            # Mezcla violeta→blanco según la fracción de la muestra que cae en
            # la onda (así las barras quedan limpias incluso a 16 px).
            wfrac = in_wave / in_mark
            rgb = tuple(
                int(round(ACCENT_RGB[i] * (1.0 - wfrac) + 255 * wfrac)) for i in range(3)
            )
            buf += bytes((rgb[0], rgb[1], rgb[2], int(round(in_mark * inv * 255))))
    return bytes(buf)


def circle_rgba(size: int) -> bytes:
    """Alias histórico (la bandeja usaba un círculo); hoy devuelve la marca."""
    return mark_rgba(size)


def _png_chunk(tag: bytes, data: bytes) -> bytes:
    """Empaqueta un chunk PNG estándar (length + tag + data + crc)."""
    length = struct.pack(">I", len(data))
    crc = struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
    return length + tag + data + crc


def png_from_rgba(rgba: bytes, size: int) -> bytes:
    """Serializa un PNG RGBA8 ``size * size`` (sin dependencias externas)."""
    # Filter byte 0 (None) por cada scanline + bytes RGBA. Para
    # descomprimirlo luego, deflate es suficiente (zlib, level 9).
    raw = bytearray()
    stride = size * 4
    for y in range(size):
        raw.append(0)  # filter: None
        raw.extend(rgba[y * stride : (y + 1) * stride])
    compressed = zlib.compress(bytes(raw), level=9)

    sig = b"\x89PNG\r\n\x1a\n"
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)  # 8-bit, RGBA
    return (
        sig
        + _png_chunk(b"IHDR", ihdr)
        + _png_chunk(b"IDAT", compressed)
        + _png_chunk(b"IEND", b"")
    )


def bmp_dib_from_rgba(rgba: bytes, size: int) -> bytes:
    """Cabecera BITMAPINFOHEADER + píxeles XOR/AND para tamaños pequeños.

    Los ICO con PNG embebido funcionan en Windows 10/11; los tamaños
    pequeños (16/24 px) usamos BMP clásico (formato DIB) para máxima
    compatibilidad con visores antiguos y con la herramienta ``rc.exe``
    (por si en algún momento queremos pre-compilar fuera de ``winresource``).
    """
    # XOR: BMP es BGRA bottom-up; AND: máscara 1 bpp (aquí toda a 0 porque
    # los píxeles ya llevan alfa y queremos respetar el antialiasing).
    xor_stride = size * 4
    xor_rows = bytearray()
    for y in range(size - 1, -1, -1):  # bottom-up
        row = bytearray(rgba[y * xor_stride : (y + 1) * xor_stride])
        # Swap R<->B por píxel (BGRA).
        for x in range(size):
            row[x * 4 + 0], row[x * 4 + 2] = row[x * 4 + 2], row[x * 4 + 0]
        xor_rows.extend(row)
    and_stride = ((size + 31) // 32) * 4  # múltiplo de 4 bytes
    and_rows = b"\x00" * (and_stride * size)  # alfa ya en XOR → AND a 0

    # BITMAPINFOHEADER (40 bytes).
    bih = struct.pack(
        "<IiiHHIIiiII",
        40,  # biSize
        size,  # biWidth
        size * 2,  # biHeight (XOR + AND)
        1,  # biPlanes
        32,  # biBitCount
        0,  # biCompression (BI_RGB)
        len(xor_rows) + len(and_rows),  # biSizeImage
        0,  # biXPelsPerMeter
        0,  # biYPelsPerMeter
        0,  # biClrUsed
        0,  # biClrImportant
    )
    return bih + bytes(xor_rows) + and_rows


def write_ico(out_path: Path, sizes: tuple[int, ...] = SIZES) -> int:
    """Escribe el archivo ``.ico`` y devuelve su tamaño en bytes.

    Para máxima compatibilidad con el cargador de recursos de Windows
    (que rechaza PNG-ICO en versiones antiguas y en herramientas como
    ``rc.exe``), todas las entradas van en formato BMP DIB (BITMAPINFOHEADER
    + XOR BGRA bottom-up + AND mask 1 bpp). El alfa lo lleva el canal A de
    XOR (sin premultiplicar), y la AND mask queda a 0 porque el shell de
    Windows 10/11 compone el alfa del XOR directamente.
    """
    # Genera cada imagen.
    images: list[tuple[int, bytes, str]] = []  # (size, bytes, fmt)
    for s in sizes:
        rgba = circle_rgba(s)
        images.append((s, bmp_dib_from_rgba(rgba, s), "bmp"))

    # Cabecera ICONDIR (6 bytes) + ICONDIRENTRY (16 bytes por imagen).
    header = struct.pack("<HHH", 0, 1, len(images))  # reserved, type=icon, count
    entries = bytearray()
    payload = bytearray()
    offset = 6 + 16 * len(images)
    for size, data, fmt in images:
        bpp = 32
        # Width/Height en la entrada: 0 significa "256" (formato ICO).
        w_field = 0 if size >= 256 else size
        h_field = 0 if size >= 256 else size
        # Plano = 1 (el campo biPlanes se ignora en ICO pero debe ser >=1
        # para que Resource Hacker y similares no se quejen).
        entry = struct.pack(
            "<BBBBHHII",
            w_field,  # bWidth
            h_field,  # bHeight
            0,  # bColorCount (0 para >8 bpp)
            1,  # bReserved (planes)
            bpp,  # wBitCount
            1,  # dwBytesInRes alto (BMP DIB)
            len(data),  # dwBytesInRes bajo
            offset,
        )
        entries += entry
        payload += data
        offset += len(data)

    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_bytes(header + bytes(entries) + bytes(payload))
    return out_path.stat().st_size


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        sys.stderr.write(f"uso: {argv[0]} <directorio_salida>\n")
        return 2
    out_dir = Path(argv[1])
    out_path = out_dir / "whispr-open.ico"
    size = write_ico(out_path)
    sys.stdout.write(
        f"icono escrito: {out_path} ({size} B, "
        f"{len(SIZES)} entradas: {','.join(str(s) for s in SIZES)})\n"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))