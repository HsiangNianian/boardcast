"""python3 scripts/site-licenses.py

Write the Rust crates the app and the website ship, grouped by license, into crates/site-gen/pages/terms.html,
between its <!-- crates --> markers. Run it after dependencies change; it reads Cargo.lock only.
"""
import collections
import html
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SHIPPED = {"boardcast-ui", "boardcast-site", "boardcast-demo"}

meta = json.loads(subprocess.run(["cargo", "metadata", "--format-version", "1", "--locked"], cwd=ROOT, check=True, capture_output=True, text=True).stdout)
packages = {p["id"]: p for p in meta["packages"]}
nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
seen, todo = set(), [i for i, p in packages.items() if p["name"] in SHIPPED]
while todo:
    i = todo.pop()
    if i in seen:
        continue
    seen.add(i)
    # Normal and build dependencies; development-only ones never ship.
    todo += [d["pkg"] for d in nodes[i]["deps"] if any(k["kind"] in (None, "build") for k in d["dep_kinds"])]

groups = collections.defaultdict(list)
for i in seen:
    p = packages[i]
    if p["name"].startswith("boardcast"):
        continue
    groups[(p.get("license") or "see its repository").replace("/", " OR ")].append(p)

out = [f'<p class="count label">{sum(map(len, groups.values()))} crates</p>']
for license, crates in sorted(groups.items(), key=lambda g: (-len(g[1]), g[0])):
    names = ", ".join(
        f'<a href="https://crates.io/crates/{html.escape(p["name"])}/{html.escape(p["version"])}">{html.escape(p["name"])}</a>'
        for p in sorted(crates, key=lambda p: (p["name"], p["version"]))
    )
    out.append(f'<h4>{html.escape(license)} <span class="label">{len(crates)}</span></h4>\n<p class="crates">{names}</p>')

page = ROOT / "crates/site-gen/pages/terms.html"
text = page.read_text()
start, end = "<!-- crates -->", "<!-- /crates -->"
head, rest = text.split(start, 1)
page.write_text(head + start + "\n" + "\n".join(out) + "\n" + end + rest.split(end, 1)[1])
print(f"{sum(map(len, groups.values()))} crates in {len(groups)} licenses")
