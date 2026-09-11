#!/usr/bin/env python3
"""Auditoria de doc-comments publicos.

Regla: todo `pub fn` / `pub struct` / `pub enum` / `pub trait` debe tener al
menos una linea `///` inmediatamente encima, saltando atributos (`#[...]`) y
la linea de continuacion del atributo. Se ignoran los bloques `#[cfg(test)]`.

Uso: python3 audit_doc_comments.py <raiz-src>
Salida: huecos por fichero + resumen por categoria.
"""
import re
import sys
from pathlib import Path

ITEM = re.compile(r"^\s*pub (fn|struct|enum|trait|type)\b")
SKIP = re.compile(r"^\s*pub (mod|const|static|use)\b")


def scan(path: Path):
    lines = path.read_text(encoding="utf-8").splitlines()
    gaps = []
    depth = 0
    test_depths = []
    pending_cfg_test = False
    in_cfg_test = set()
    for i, raw in enumerate(lines):
        stripped = raw.strip()
        if stripped.startswith("#[cfg(test)]"):
            pending_cfg_test = True
        if ITEM.match(raw) or SKIP.match(raw):
            inside = any(depth >= d for d in test_depths) or any(
                i in r for r in in_cfg_test
            )
            if not inside and ITEM.match(raw):
                # buscar hacia arriba saltando atributos
                j = i - 1
                while j >= 0 and (lines[j].strip().startswith("#[") or lines[j].strip() == ""):
                    if lines[j].strip() == "":
                        break
                    j -= 1
                if not (j >= 0 and lines[j].strip().startswith("///")):
                    gaps.append((i + 1, stripped[:110]))
        opens, closes = raw.count("{"), raw.count("}")
        if pending_cfg_test and "{" in raw:
            test_depths.append(depth + raw.index("{"))
            pending_cfg_test = False
        depth += opens - closes
        test_depths = [d for d in test_depths if d < depth]
    return gaps


def main():
    root = Path(sys.argv[1] if len(sys.argv) > 1 else "src")
    total = 0
    for f in sorted(root.rglob("*.rs")):
        gaps = [g for g in scan(f) if not g[1].startswith("pub fn")]
        fns = [g for g in scan(f) if g[1].startswith("pub fn")]
        if fns:
            print(f"\n{f}  [pub fn sin doc: {len(fns)}]")
            for line, text in fns:
                print(f"  {line:5d}: {text}")
        if gaps:
            print(f"\n{f}  [pub tipo sin doc: {len(gaps)}]")
            for line, text in gaps:
                print(f"  {line:5d}: {text}")
        total += len(fns) + len(gaps)
    print(f"\nTOTAL huecos: {total}")


if __name__ == "__main__":
    main()
