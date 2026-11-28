"""Regenerates the sky of the client (assets/sky/sky.png).

    python tools/sky.py [--cache DIR] [--preview FILE]

Source: Poly Haven "Kloofendal 48d Partly Cloudy (Pure Sky)", CC0 1.0 (public
domain, no attribution required; listed anyway in assets/sky/SOURCES.md).

The equirectangular HDR becomes a cubemap of six 512 px faces stacked
vertically in wgpu order (+X, -X, +Y, -Y, +Z, -Z), as `crates/ps-client/src/sky.rs`
loads it:
  - turned so that its sun sits where the map's sun light comes from
    (`SUN_DIR`, same as `SUN_OFFSET` in `world.rs`);
  - exposed so the bright sky (sun excluded) lands just under white, softened a
    little and slightly desaturated, to stay calm behind flat matte colours
    (docs/brand.md).

Needs Pillow and NumPy (`pip install pillow numpy`); the download goes to
--cache (kept between runs).
"""

import argparse
import math
import urllib.request
from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "assets" / "sky"
ASSET = "kloofendal_48d_partly_cloudy_puresky"
URL = f"https://dl.polyhaven.org/file/ph-assets/HDRIs/hdr/2k/{ASSET}_2k.hdr"
FACE = 512
# Direction the sun light comes from, render space (Y up), see world.rs.
SUN_DIR = np.array([20.0, 60.0, 30.0]) / math.sqrt(20**2 + 60**2 + 30**2)
SATURATION = 0.8


def read_hdr(path: Path) -> np.ndarray:
    """Radiance RGBE (new run-length encoding) to a float32 H×W×3 array."""
    data = path.read_bytes()
    pos = data.index(b"\n\n") + 2
    end = data.index(b"\n", pos)
    size = data[pos:end].split()
    assert size[0] == b"-Y" and size[2] == b"+X", size
    h, w = int(size[1]), int(size[3])
    pos = end + 1
    rgbe = np.empty((h, w, 4), np.uint8)
    for y in range(h):
        assert data[pos] == 2 and data[pos + 1] == 2, "old RLE not supported"
        pos += 4
        for c in range(4):
            x = 0
            row = rgbe[y, :, c]
            while x < w:
                n = data[pos]
                pos += 1
                if n > 128:
                    n -= 128
                    row[x : x + n] = data[pos]
                    pos += 1
                else:
                    row[x : x + n] = np.frombuffer(data, np.uint8, n, pos)
                    pos += n
                x += n
    e = rgbe[..., 3].astype(np.int32)
    scale = np.where(e > 0, np.ldexp(1.0, e - 136), 0.0)
    return (rgbe[..., :3] * scale[..., None]).astype(np.float32)


def face_dirs(face: int) -> np.ndarray:
    """World directions of the texels of one cube face (wgpu face order).

    The Bevy skybox shader samples the cube with z negated (left-handed cube
    maps), hence the final flip."""
    t, s = np.meshgrid(
        (np.arange(FACE) + 0.5) / FACE * 2 - 1,
        (np.arange(FACE) + 0.5) / FACE * 2 - 1,
        indexing="ij",
    )
    one = np.ones_like(s)
    c = [
        (one, -t, -s),
        (-one, -t, s),
        (s, one, t),
        (s, -one, -t),
        (s, -t, one),
        (-s, -t, -one),
    ][face]
    d = np.stack([c[0], c[1], -c[2]], -1)
    return d / np.linalg.norm(d, axis=-1, keepdims=True)


def lookup(img: np.ndarray, d: np.ndarray, yaw: float) -> np.ndarray:
    h, w, _ = img.shape
    lon = np.arctan2(d[..., 0], -d[..., 2]) + yaw
    lat = np.arcsin(np.clip(d[..., 1], -1, 1))
    x = ((lon / (2 * math.pi) + 0.5) % 1.0) * w
    y = np.clip((0.5 - lat / math.pi) * h, 0, h - 1)
    # Bilinear.
    x0, y0 = np.floor(x - 0.5).astype(int), np.floor(y - 0.5).astype(int)
    fx, fy = (x - 0.5 - x0)[..., None], (y - 0.5 - y0)[..., None]
    xa, xb = x0 % w, (x0 + 1) % w
    ya, yb = np.clip(y0, 0, h - 1), np.clip(y0 + 1, 0, h - 1)
    top = img[ya, xa] * (1 - fx) + img[ya, xb] * fx
    bot = img[yb, xa] * (1 - fx) + img[yb, xb] * fx
    return top * (1 - fy) + bot * fy


def sun_lon(img: np.ndarray) -> float:
    """Longitude of the brightest texel (the sun), as `lookup` measures it."""
    lum = img @ np.array([0.2126, 0.7152, 0.0722], np.float32)
    y, x = np.unravel_index(np.argmax(lum), lum.shape)
    return (x / lum.shape[1] - 0.5) * 2 * math.pi


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cache", default=str(ROOT / "target" / "sky-sources"))
    ap.add_argument("--preview", help="PNG with the faces unfolded as a cross")
    a = ap.parse_args()
    cache = Path(a.cache)
    cache.mkdir(parents=True, exist_ok=True)
    src = cache / f"{ASSET}_2k.hdr"
    if not src.exists():
        req = urllib.request.Request(URL, headers={"User-Agent": "project-s-sky"})
        with urllib.request.urlopen(req, timeout=300) as r:
            src.write_bytes(r.read())
    hdr = read_hdr(src)

    # Turn the panorama so its sun longitude matches SUN_DIR.
    target = math.atan2(SUN_DIR[0], -SUN_DIR[2])
    yaw = sun_lon(hdr) - target

    faces = [lookup(hdr, face_dirs(f), yaw) for f in range(6)]
    lum = np.concatenate([f.reshape(-1, 3) for f in faces]) @ [0.2126, 0.7152, 0.0722]
    exposure = 0.92 / np.percentile(lum, 99.0)
    out = []
    for f in faces:
        c = f * exposure
        grey = (c @ np.array([0.2126, 0.7152, 0.0722]))[..., None]
        c = np.clip(grey + (c - grey) * SATURATION, 0, 1)
        srgb = np.where(c <= 0.0031308, c * 12.92, 1.055 * c ** (1 / 2.4) - 0.055)
        im = Image.fromarray((srgb * 255 + 0.5).astype(np.uint8), "RGB")
        out.append(im.filter(ImageFilter.GaussianBlur(0.8)))

    OUT.mkdir(parents=True, exist_ok=True)
    stack = Image.new("RGB", (FACE, FACE * 6))
    for i, im in enumerate(out):
        stack.paste(im, (0, FACE * i))
    stack.save(OUT / "sky.png", optimize=True)
    print(f"sky.png: {ASSET}, yaw {math.degrees(yaw):.1f}°, exposure {exposure:.3f}")

    if a.preview:
        # Cross: +Y on top, then -X +Z +X -Z (looking around), -Y below.
        cross = Image.new("RGB", (FACE * 4, FACE * 3), (18, 20, 23))
        for i, (cx, cy) in enumerate([(2, 1), (0, 1), (1, 0), (1, 2), (1, 1), (3, 1)]):
            cross.paste(out[i], (cx * FACE, cy * FACE))
        cross.resize((FACE * 2, FACE * 3 // 2)).save(a.preview)


if __name__ == "__main__":
    main()
