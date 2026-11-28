# Ciel

`sky.png` : cubemap de six faces de 512 px empilées verticalement (ordre wgpu +X, -X, +Y,
-Y, +Z, -Z), générée par `python tools/sky.py` à partir du panorama HDR 2K
[Kloofendal 48d Partly Cloudy (Pure Sky)](https://polyhaven.com/a/kloofendal_48d_partly_cloudy_puresky)
de Poly Haven (Greg Zaal, Jarod Guest), sous licence **CC0 1.0** (domaine public : ni
attribution ni restriction ; liste tenue pour la traçabilité).

Le panorama est tourné pour que son soleil soit dans la direction de la lumière du soleil de
la carte (`SUN_OFFSET` dans `crates/ps-client/src/world.rs`), exposé pour que le ciel clair
reste sous le blanc, légèrement adouci et désaturé. Le client l'embarque dans le binaire
(`crates/ps-client/src/sky.rs`) comme skybox de la caméra principale.
