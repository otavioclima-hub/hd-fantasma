#!/usr/bin/env python3
"""Generates THIRD_PARTY_LICENSES.md (Rust crates shipped on macOS and Windows + npm production deps).

Run from anywhere:  python3 scripts/gen-third-party-licenses.py
Needs: cargo (rustup on PATH) and `npm install` already done (reads node_modules).
Only normal (runtime) dependencies of the app and the MCP are listed; build/dev tools are not shipped.
"""
import glob, json, os, subprocess, sys, collections

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
os.environ["PATH"] = "/opt/homebrew/opt/rustup/bin:" + os.path.expanduser("~/.cargo/bin") + ":" + os.environ["PATH"]
TARGETS = ["aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-pc-windows-msvc"]
OWN = {"hd-fantasma", "hdf-core", "hd-fantasma-mcp"}
LIC_PATTERNS = ["LICEN[SC]E*", "COPYING*", "NOTICE*", "UNLICENSE*", "license*", "copying*", "notice*"]


def rust_packages():
    found = {}
    for t in TARGETS:
        out = subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--filter-platform", t, "--locked"],
            cwd=ROOT, capture_output=True, text=True, check=True).stdout
        md = json.loads(out)
        pk = {p["id"]: p for p in md["packages"]}
        nodes = {n["id"]: n for n in md["resolve"]["nodes"]}
        roots = [i for i, p in pk.items() if p["name"] in ("hd-fantasma", "hd-fantasma-mcp")]
        seen, stack = set(), list(roots)
        while stack:
            i = stack.pop()
            if i in seen:
                continue
            seen.add(i)
            for d in nodes[i]["deps"]:
                # keep only normal deps (kind None); skip build/dev
                if any(k["kind"] is None for k in d["dep_kinds"]):
                    stack.append(d["pkg"])
        for i in seen:
            p = pk[i]
            if p["name"] in OWN:
                continue
            found[(p["name"], p["version"])] = p
    return found


def read_license_files(d):
    files = []
    for pat in LIC_PATTERNS:
        for f in sorted(glob.glob(os.path.join(d, pat))):
            if os.path.isfile(f) and f not in files:
                files.append(f)
    # also look one level into a license/ dir
    for sub in ("license", "licenses", "LICENSES"):
        for f in sorted(glob.glob(os.path.join(d, sub, "*"))):
            if os.path.isfile(f) and f not in files:
                files.append(f)
    out = []
    for f in files:
        try:
            out.append((os.path.basename(f), open(f, encoding="utf-8", errors="replace").read().strip()))
        except OSError:
            pass
    return out


def npm_packages():
    """Production deps + devDependencies whose code is bundled into the frontend (svelte, sveltekit runtime)."""
    pkg = json.load(open(os.path.join(ROOT, "package.json")))
    roots = list(pkg.get("dependencies", {})) + ["svelte", "@sveltejs/kit"]
    prod, stack = {}, list(roots)
    while stack:
        name = stack.pop()
        d = os.path.join(ROOT, "node_modules", name)
        pj = os.path.join(d, "package.json")
        if not os.path.exists(pj):
            continue
        meta = json.load(open(pj))
        key = (name, meta.get("version"))
        if key in prod:
            continue
        prod[key] = (d, meta)
        stack.extend(meta.get("dependencies", {}))
    return prod


def lic_of(meta):
    l = meta.get("license")
    if isinstance(l, dict):
        l = l.get("type")
    if not l and meta.get("licenses"):
        l = " OR ".join(x.get("type", "?") for x in meta["licenses"])
    return l or "UNKNOWN"


MIT = """MIT License

Copyright (c) the authors listed for each package above

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and
to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of
the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO
THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE."""


def main():
    rust = rust_packages()
    npm = npm_packages()
    texts = collections.OrderedDict()  # text -> list of "name version"
    rows = []
    missing = []

    def add(kind, name, ver, lic, repo, d, authors=''):
        files = read_license_files(d)
        if not files:
            missing.append((name, ver, lic, authors, repo))
        for fname, txt in files:
            texts.setdefault(txt, []).append(f"{name} {ver}")
        rows.append((kind, name, ver, lic, repo))

    for (name, ver), p in sorted(rust.items()):
        add("Rust", name, ver, p.get("license") or p.get("license_file") or "UNKNOWN",
            p.get("repository") or p.get("homepage") or "", os.path.dirname(p["manifest_path"]),
            ", ".join(p.get("authors") or []))
    for (name, ver), (d, meta) in sorted(npm.items()):
        repo = meta.get("repository")
        repo = repo.get("url") if isinstance(repo, dict) else (repo or "")
        au = meta.get("author")
        au = au.get("name") if isinstance(au, dict) else (au or "")
        add("npm", name, ver, lic_of(meta), repo, d, au)

    by_lic = collections.Counter(r[3] for r in rows)
    o = []
    o.append("# Avisos de terceiros / Third-party notices\n")
    o.append("HD Fantasma is MIT-licensed (see `LICENSE`). It includes the open-source components below, "
             "each under its own license. / O HD Fantasma é MIT (veja `LICENSE`) e inclui os componentes "
             "abertos abaixo, cada um com a sua licença.\n")
    o.append("Generated by `scripts/gen-third-party-licenses.py` from `Cargo.lock` (macOS and Windows targets, runtime "
             "dependencies of the app and of `hd-fantasma-mcp`) and `package-lock.json` (production dependencies).\n")
    o.append("## Summary / Resumo\n")
    o.append("| License | Packages |\n|---|---|")
    for l, c in by_lic.most_common():
        o.append(f"| {l} | {c} |")
    o.append("\n## Packages / Pacotes\n")
    o.append("| Type | Package | Version | License | Source |\n|---|---|---|---|---|")
    for kind, name, ver, lic, repo in rows:
        o.append(f"| {kind} | {name} | {ver} | {lic} | {repo} |")
    o.append("\n## License texts / Textos das licenças\n")
    o.append("Identical texts are shown once, followed by the packages that use them. / "
             "Textos idênticos aparecem uma vez, seguidos dos pacotes que os usam.\n")
    for txt, pkgs in texts.items():
        o.append("---\n")
        o.append("**Used by:** " + ", ".join(sorted(set(pkgs))) + "\n")
        o.append("```text\n" + txt.replace("```", "'''") + "\n```\n")
    if missing:
        o.append("---\n\n## Packages whose license file is not shipped in the package / Pacotes sem arquivo de licença no pacote\n")
        o.append("These declare their license by SPDX identifier in the package metadata. The standard text of "
                 "that license (https://spdx.org/licenses/) applies, with the copyright of the authors listed. "
                 "For dual/triple-licensed packages (OR) we use the MIT option. / "
                 "Estes declaram a licença só pelo identificador SPDX; vale o texto padrão dela, com o copyright dos autores listados.\n")
        for name, ver, lic, authors, repo in missing:
            o.append(f"- **{name} {ver}** - {lic} - {authors or 'authors as listed in the project repository'} - {repo}")
        o.append("\n`selectors` is under MPL-2.0: its source code is available unmodified at "
                 "https://github.com/servo/stylo and the license text at https://www.mozilla.org/MPL/2.0/.\n")
        o.append("Standard MIT license text used for the MIT / MIT-option packages above:\n")
        o.append("```text\n" + MIT + "\n```\n")
    open(os.path.join(ROOT, "THIRD_PARTY_LICENSES.md"), "w").write("\n".join(o))
    print(f"rust={len(rust)} npm={len(npm)} distinct texts={len(texts)} missing files={len(missing)}")
    for m in missing:
        print("  no file:", m[:3])


if __name__ == "__main__":
    sys.exit(main())
