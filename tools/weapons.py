"""Regenerates the weapon models of the client (assets/weapons/*.bin).

    python tools/weapons.py [--cache DIR] [--preview FILE]

Source: Quaternius "Ultimate Gun Pack" (OpenGameArt), CC0 1.0 (public domain,
no attribution required; listed anyway in assets/weapons/SOURCES.md). The OBJ
exports are used: flat low-poly shapes with one plain colour per material.

Each model is fitted to the conventions of `ps-engine-character::weapon_mesh`
(which it replaces for the holds listed below; the grenade and the bomb stay
procedural):
  - origin where the right hand grips, -z towards the muzzle, game units
    (1 u = 2.54 cm), real-world length;
  - colours remapped onto the matte gunmetal palette of the engine, so a weapon
    always reads apart from the paint body (docs/brand.md: flat matte colours).

Output, little-endian: b"PSW1", u8 colour count, colours as 3 f32 (linear),
u32 triangle count, then per triangle a u8 colour index and 3 vertices of
3 f32. Normals are flat and computed by the client.

Needs only the standard library (Pillow for --preview); the download goes to
--cache (kept between runs).
"""

import argparse
import io
import struct
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "assets" / "weapons"
URL = "https://opengameart.org/sites/default/files/ultimate_gun_pack_by_quaternius.zip"
OBJ_DIR = "Ultimate Gun Pack - July 2019/OBJ/"

# Linear colours, same family as ps-engine-character::weapon_mesh.
DARK = (0.07, 0.075, 0.085)
MID = (0.16, 0.17, 0.19)
STEEL = (0.26, 0.27, 0.29)
LIGHT = (0.42, 0.44, 0.47)
BLADE = (0.7, 0.72, 0.75)
LENS = (0.05, 0.07, 0.12)
PALETTE = {
    "Black": DARK,
    "MainDark": DARK,
    "Main": MID,
    "DarkMetal": MID,
    "Grey": MID,
    "Metal": STEEL,
    "MainLight": STEEL,
    "LightMetal": LIGHT,
    "LightMetal2": BLADE,
    "Glass": LENS,
}

# Output name, source OBJ, grip point in source coordinates (x = muzzle side,
# y up), total length in game units, optional box of the source (min, max):
# polygons centred in it are dropped (the bipod hanging under the sniper).
WEAPONS = [
    ("rifle", "AssaultRifle2_1", (0.0, 0.22), 33.0, None),
    (
        "sniper",
        "SniperRifle_5",
        (-0.68, 0.13),
        43.0,
        ((1.35, -1.0, -1.0), (1.85, 0.31, 1.0)),
    ),
    ("pistol", "Pistol_5", (-0.04, 0.12), 7.6, None),
    ("knife", "Accessories/Bayonet", (0.05, 0.0), 12.0, None),
]


def pack(cache: Path) -> zipfile.ZipFile:
    archive = cache / "ultimate_gun_pack_by_quaternius.zip"
    if not archive.exists():
        req = urllib.request.Request(URL, headers={"User-Agent": "project-s-weapons"})
        with urllib.request.urlopen(req, timeout=300) as r:
            archive.write_bytes(r.read())
    return zipfile.ZipFile(archive)


def load(z: zipfile.ZipFile, name: str):
    """Polygons of an OBJ as (points, material)."""
    obj = io.TextIOWrapper(z.open(OBJ_DIR + name + ".obj"), encoding="utf-8")
    verts, polys, mat = [], [], None
    for line in obj:
        t = line.split()
        if not t:
            continue
        if t[0] == "v":
            verts.append(tuple(float(x) for x in t[1:4]))
        elif t[0] == "usemtl":
            mat = t[1]
        elif t[0] == "f":
            polys.append(([verts[int(x.split("/")[0]) - 1] for x in t[1:]], mat))
    return polys


def convert(polys, grip, length, drop):
    if drop:
        lo, hi = drop
        def centred_in_box(pts):
            c = [sum(p[i] for p in pts) / len(pts) for i in range(3)]
            return all(lo[i] <= c[i] <= hi[i] for i in range(3))

        polys = [(pts, m) for pts, m in polys if not centred_in_box(pts)]
    xs = [p[0] for pts, _ in polys for p in pts]
    scale = length / (max(xs) - min(xs))
    colours, tris = [], []
    for pts, mat in polys:
        col = PALETTE[mat]
        if col not in colours:
            colours.append(col)
        # Source x (muzzle) -> -z, z -> x: a rotation, so the winding stays.
        q = [
            (p[2] * scale, (p[1] - grip[1]) * scale, -(p[0] - grip[0]) * scale)
            for p in pts
        ]
        for i in range(1, len(q) - 1):
            tris.append((colours.index(col), (q[0], q[i], q[i + 1])))
    return colours, tris


def write(path: Path, colours, tris):
    out = bytearray(b"PSW1")
    out += struct.pack("<B", len(colours))
    for c in colours:
        out += struct.pack("<3f", *c)
    out += struct.pack("<I", len(tris))
    for ci, tri in tris:
        out += struct.pack("<B", ci)
        for v in tri:
            out += struct.pack("<3f", *v)
    path.write_bytes(bytes(out))


def preview(path: Path, models):
    """Side views (+x right = towards the muzzle), grip marked in red."""
    from PIL import Image, ImageDraw

    s, w, h = 14, 640, 170
    img = Image.new("RGB", (w, h * len(models)), (18, 20, 23))
    d = ImageDraw.Draw(img)
    for row, (name, colours, tris) in enumerate(models):
        ox, oy = w * 0.55, h * row + h * 0.55
        srgb = lambda c: int(255 * min(1.0, (1.055 * c ** (1 / 2.4) - 0.055)))
        for ci, tri in sorted(tris, key=lambda t: sum(v[0] for v in t[1])):
            d.polygon(
                [(ox - v[2] * s, oy - v[1] * s) for v in tri],
                fill=tuple(srgb(c) for c in colours[ci]),
            )
        d.ellipse((ox - 3, oy - 3, ox + 3, oy + 3), fill=(240, 97, 42))
        d.text((6, h * row + 4), name, fill=(236, 232, 225))
    img.save(path)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cache", default=str(ROOT / "target" / "weapon-sources"))
    ap.add_argument("--preview", help="PNG with a side view of every model")
    a = ap.parse_args()
    cache = Path(a.cache)
    cache.mkdir(parents=True, exist_ok=True)
    OUT.mkdir(parents=True, exist_ok=True)
    z = pack(cache)
    models = []
    for name, src, grip, length, drop in WEAPONS:
        colours, tris = convert(load(z, src), grip, length, drop)
        write(OUT / f"{name}.bin", colours, tris)
        models.append((name, colours, tris))
        print(f"{name}: {src}, {len(tris)} triangles")
    if a.preview:
        preview(Path(a.preview), models)


if __name__ == "__main__":
    main()
