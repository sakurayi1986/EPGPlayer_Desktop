#!/usr/bin/env python3
"""Write the license texts of every Rust crate linked into the program.

    python3 scripts/gen-third-party.py x86_64-unknown-linux-gnu > THIRD_PARTY_LICENSES.txt

Only normal (non-dev, non-build) dependencies for the given target are listed.
Run `cargo fetch` first so every crate's sources are in the local registry.
"""
import json
import subprocess
import sys
from pathlib import Path

LICENSE_PREFIXES = ("license", "licence", "copying", "notice", "copyright")


def main():
    target = sys.argv[1] if len(sys.argv) > 1 else "x86_64-unknown-linux-gnu"
    meta = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--filter-platform", target]))
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = meta["resolve"]["root"]

    seen, stack = set(), [root]
    while stack:
        pid = stack.pop()
        if pid in seen:
            continue
        seen.add(pid)
        for dep in nodes[pid]["deps"]:
            if any(k["kind"] is None for k in dep["dep_kinds"]):
                stack.append(dep["pkg"])
    seen.discard(root)

    out = sys.stdout
    printed = {}  # text -> crate that printed it first (Apache-2.0 etc. repeat hundreds of times)
    out.write("Third-party Rust crates linked into EPGPlayer For Desktop\n")
    out.write(f"Target: {target}\n")
    out.write(f"{len(seen)} crates\n\n")
    for p in sorted((packages[i] for i in seen), key=lambda p: (p["name"], p["version"])):
        out.write("=" * 78 + "\n")
        out.write(f"{p['name']} {p['version']}\n")
        out.write(f"License: {p.get('license') or 'see below'}\n")
        if p.get("repository"):
            out.write(f"Source: {p['repository']}\n")
        out.write("=" * 78 + "\n\n")
        crate_dir = Path(p["manifest_path"]).parent
        files = sorted(
            f for f in crate_dir.iterdir()
            if f.is_file() and f.name.lower().startswith(LICENSE_PREFIXES)
        )
        if p.get("license_file"):
            lf = crate_dir / p["license_file"]
            if lf.is_file() and lf not in files:
                files.append(lf)
        if not files:
            out.write(f"(No license file in the crate; licensed under {p.get('license')}.)\n\n")
        for f in files:
            text = f.read_text(encoding="utf-8", errors="replace").rstrip()
            out.write(f"--- {f.name} ---\n")
            first = printed.setdefault(text, f"{p['name']} {p['version']} ({f.name})")
            if first.startswith(f"{p['name']} {p['version']} "):
                out.write(text + "\n\n")
            else:
                out.write(f"(Identical to the text printed above for {first}.)\n\n")


if __name__ == "__main__":
    main()
