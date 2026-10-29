#!/usr/bin/env python3
"""Genera las imágenes de marca de la documentación a partir del icono real.

- ``docs/img/logo.png``: el icono de la app (256 px, sacado del .ico que genera
  ``assets/build_icon.py``).
- ``docs/img/banner.png``: cabecera del README, con el mismo icono, el nombre de
  la app en Inter y los colores de la interfaz (fondo #15151B, violeta #7C6CFF).

Se ejecuta a mano cuando cambie el icono o el nombre:

    python scripts/make-brand-images.py
"""

import pathlib
import sys

from PIL import Image, ImageDraw, ImageFont

ROOT = pathlib.Path(__file__).resolve().parent.parent
ICO = ROOT / "assets" / "whispr-open.ico"
OUT = ROOT / "docs" / "img"
FONT_BOLD = ROOT / "assets" / "fonts" / "Inter-SemiBold.ttf"
FONT_REG = ROOT / "assets" / "fonts" / "Inter-Regular.ttf"

BG = (21, 21, 27)        # fondo de la app
PURPLE = (124, 108, 255)  # #7C6CFF, el violeta de la app
TEXT = (240, 240, 245)
MUTED = (160, 160, 175)


def icon(size: int) -> Image.Image:
    """El icono de la app al tamaño pedido, con transparencia."""
    img = Image.open(ICO)
    # El .ico trae varias resoluciones; se elige la más cercana y se escala.
    best = min(img.ico.sizes(), key=lambda s: abs(s[0] - size))
    img.size = best
    return img.convert("RGBA").resize((size, size), Image.LANCZOS)


def make_logo() -> pathlib.Path:
    path = OUT / "logo.png"
    icon(256).save(path)
    return path


def make_banner(width: int = 1280, height: int = 380) -> pathlib.Path:
    img = Image.new("RGB", (width, height), BG)
    d = ImageDraw.Draw(img)

    # Franja violeta abajo, el mismo acento que usa la app en la cabecera.
    d.rectangle([0, height - 6, width, height], fill=PURPLE)

    # Icono a la izquierda.
    logo = icon(190)
    img.paste(logo, (110, (height - 190) // 2), logo)

    x = 360
    name = ImageFont.truetype(str(FONT_BOLD), 118)
    d.text((x, 108), "Silb", font=name, fill=TEXT)

    tagline = ImageFont.truetype(str(FONT_REG), 34)
    d.text((x, 246), "Dictado por voz local, en tu propio equipo",
           font=tagline, fill=MUTED)

    # Palabra «local» en violeta, que es lo que vende la app.
    start = x + d.textlength("Dictado por voz ", font=tagline)
    d.text((start, 246), "local", font=tagline, fill=PURPLE)

    path = OUT / "banner.png"
    img.save(path)
    return path


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    if not ICO.exists():
        sys.exit(f"falta el icono: {ICO}")
    for p in (make_logo(), make_banner()):
        print(f"ok {p.relative_to(ROOT)} ({p.stat().st_size} B)")
