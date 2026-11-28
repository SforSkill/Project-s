# Textures de surface

Cartes de détail mates (luminance seule, 512 px) générées par `python tools/textures.py`
à partir des cartes de couleur 1K d'[ambientCG](https://ambientcg.com), sous licence
**CC0 1.0** (domaine public : ni attribution ni restriction ; liste tenue pour la traçabilité).
Le client les multiplie par la couleur du matériau de Project-s-content
(`configs/materials.toml`) et les embarque dans le binaire (`crates/ps-client/src/textures.rs`).

| Fichier | Matériau | Source ambientCG |
|---|---|---|
| `concrete.png` | concrete | [Concrete034](https://ambientcg.com/view?id=Concrete034) |
| `concrete_dark.png` | concrete_dark | [Concrete031](https://ambientcg.com/view?id=Concrete031) |
| `brick.png` | brick | [Bricks060](https://ambientcg.com/view?id=Bricks060) |
| `plaster.png` | plaster | [Plaster001](https://ambientcg.com/view?id=Plaster001) |
| `wood.png` | wood | [Planks037A](https://ambientcg.com/view?id=Planks037A) |
| `crate.png` | crate | [Planks023A](https://ambientcg.com/view?id=Planks023A) |
| `metal_thin.png` | metal_thin | [CorrugatedSteel009](https://ambientcg.com/view?id=CorrugatedSteel009) |
| `metal_thick.png` | metal_thick | [Metal032](https://ambientcg.com/view?id=Metal032) |
| `sand.png` | sand | [Ground054](https://ambientcg.com/view?id=Ground054) |
| `grass.png` | grass | [Grass004](https://ambientcg.com/view?id=Grass004) |
| `tile.png` | tile | [Tiles139](https://ambientcg.com/view?id=Tiles139) |

`glass`, `target_orange`, `site_a`, `site_b` restent en aplat (damier de 64 u conservé).
Changer une source : modifier `TEXTURES` dans `tools/textures.py`, relancer le script ;
un nouveau matériau s'ajoute aussi à `TEXTURES` dans `textures.rs` (taille de répétition).
