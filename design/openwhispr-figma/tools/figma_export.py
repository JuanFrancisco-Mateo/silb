"""Descarga los assets SVG referenciados por los design-context de Figma + saca los tokens.

Uso: python figma_export.py <carpeta-figma-dump> <carpeta-destino>
"""
import json
import pathlib
import re
import subprocess
import sys
from collections import Counter

TOKENS = os.environ.get("FIGMA_TOKENS") or os.path.expanduser("~/.config/figma/tokens.json")


def main(src, dst):
    src, dst = pathlib.Path(src), pathlib.Path(dst)
    icons = dst / "icons"
    icons.mkdir(parents=True, exist_ok=True)
    token = json.load(open(TOKENS, encoding="utf-8"))["access_token"]

    seen, ok, fail = set(), 0, []
    colors, sizes = Counter(), Counter()
    for f in sorted(src.glob("*get_design_context.txt")):
        text = f.read_text(encoding="utf-8")
        prefix = re.search(r'assetPathPrefix = "([^"]+)"', text)
        prefix = prefix.group(1) if prefix else ""
        for name, path in re.findall(r"const (img\w+) = `\$\{assetPathPrefix\}/([^`]+)`", text):
            url = f"{prefix}/{path}"
            out = icons / f"{name[3:].lower()}-{path}"
            if url in seen:
                continue
            seen.add(url)
            try:
                data = subprocess.run(["curl", "-sL", "-H", f"Authorization: Bearer {token}", url],
                                      capture_output=True, timeout=30).stdout
                if not data:
                    raise RuntimeError("respuesta vacia")
                out.write_bytes(data)
                ok += 1
            except Exception as e:  # noqa: BLE001
                fail.append(f"{name}: {e}")
        colors.update(re.findall(r"#[0-9a-fA-F]{6}", text))
        sizes.update(re.findall(r"text-\[(\d+)px\]", text))

    out_md = dst / "TOKENS.md"
    lines = ["# Tokens extraidos del diseno de Figma (recuento de usos)\n", "## Colores\n"]
    lines += [f"- `{c}` x{n}" for c, n in colors.most_common()]
    lines += ["\n## Tamanos de fuente\n"]
    lines += [f"- {s}px x{n}" for s, n in sorted(sizes.items(), key=lambda kv: -kv[1])]
    out_md.write_text("\n".join(lines), encoding="utf-8")
    print(f"iconos OK={ok} fallos={len(fail)} -> {icons}")
    for f_ in fail:
        print("  !", f_)
    print("tokens ->", out_md)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
