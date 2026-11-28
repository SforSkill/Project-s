"""Regenerates the surface textures of the client (assets/textures/*.png).

    python tools/textures.py [--cache DIR]

Sources: ambientCG materials, CC0 1.0 (public domain, no attribution required;
listed anyway in assets/textures/SOURCES.md). Only the colour map is used.

Each colour map becomes a matte *detail map* that follows the art direction
(docs/brand.md: flat matte colours, 3D that reads as 2D):
  - luminance only: the colour stays the one of Project-s-content
    (configs/materials.toml), which the renderer multiplies with the texture;
  - softened then posterised into a few flat steps, at a low contrast chosen per
    material (bricks and tiles need their joints, concrete almost none);
  - mean brightness ~0.9 (linear), so a material keeps the shade of its colour.

Needs Pillow (`pip install pillow`); downloads go to --cache (kept between runs).
"""

import argparse
import io
import math
import urllib.request
import zipfile
from pathlib import Path

from PIL import Image, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "assets" / "textures"
SIZE = 512
MEAN = 0.9  # linear brightness the detail map averages to
STEPS = 7  # flat tones per texture

# material (configs/materials.toml) -> (ambientCG asset, contrast = linear std dev)
TEXTURES = {
    "concrete": ("Concrete034", 0.05),
    "concrete_dark": ("Concrete031", 0.06),
    "brick": ("Bricks060", 0.12),
    "plaster": ("Plaster001", 0.04),
    "wood": ("Planks037A", 0.09),
    "crate": ("Planks023A", 0.10),
    "metal_thin": ("CorrugatedSteel009", 0.08),
    "metal_thick": ("Metal032", 0.06),
    "sand": ("Ground054", 0.06),
    "grass": ("Grass004", 0.08),
    "tile": ("Tiles139", 0.10),
}


def srgb_to_linear(c: float) -> float:
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def linear_to_srgb(c: float) -> float:
    return c * 12.92 if c <= 0.0031308 else 1.055 * c ** (1 / 2.4) - 0.055


def colour_map(asset: str, cache: Path) -> Image.Image:
    archive = cache / f"{asset}_1K-PNG.zip"
    if not archive.is_file():
        url = f"https://ambientcg.com/get?file={asset}_1K-PNG.zip"
        req = urllib.request.Request(url, headers={"User-Agent": "project-s-textures"})
        with urllib.request.urlopen(req, timeout=120) as r:
            archive.write_bytes(r.read())
    with zipfile.ZipFile(archive) as z:
        name = next(n for n in z.namelist() if n.endswith("_Color.png"))
        return Image.open(io.BytesIO(z.read(name))).convert("RGB")


def stylise(img: Image.Image, contrast: float) -> Image.Image:
    lum = img.convert("L").resize((SIZE, SIZE), Image.LANCZOS)
    lum = lum.filter(ImageFilter.MedianFilter(5))  # flattens grain into patches
    lin = [srgb_to_linear(v / 255) for v in range(256)]
    hist = lum.histogram()
    n = sum(hist)
    mean = sum(c * lin[v] for v, c in enumerate(hist)) / n
    std = math.sqrt(sum(c * (lin[v] - mean) ** 2 for v, c in enumerate(hist)) / n) or 1.0
    lo = MEAN - 3 * contrast
    step = 6 * contrast / (STEPS - 1)
    lut = []
    for v in range(256):
        t = MEAN + (lin[v] - mean) / std * contrast
        t = lo + round((t - lo) / step) * step  # posterise into flat steps
        t = min(max(t, lo), min(MEAN + 3 * contrast, 1.0))
        lut.append(round(linear_to_srgb(t) * 255))
    return lum.point(lut).convert("RGB")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cache", default=str(ROOT / "target" / "texture-sources"))
    a = ap.parse_args()
    cache = Path(a.cache)
    cache.mkdir(parents=True, exist_ok=True)
    OUT.mkdir(parents=True, exist_ok=True)
    for material, (asset, contrast) in TEXTURES.items():
        out = OUT / f"{material}.png"
        stylise(colour_map(asset, cache), contrast).save(out, optimize=True)
        print(f"{out.relative_to(ROOT)}  <- {asset}  ({math.ceil(out.stat().st_size / 1024)} Kio)")


if __name__ == "__main__":
    main()
