# Modèles d'armes

Modèles low-poly générés par `python tools/weapons.py` à partir des exports OBJ de
l'[Ultimate Gun Pack](https://opengameart.org/content/low-poly-guns-pack) de Quaternius,
sous licence **CC0 1.0** (domaine public : ni attribution ni restriction ; liste tenue pour
la traçabilité). Le client les embarque dans le binaire (`crates/ps-client/src/character/weapon_models.rs`)
et les utilise à la place des maillages procéduraux de `ps-engine-character::weapon_mesh`.

| Fichier | Prise (`Hold`) | Modèle source | Longueur |
|---|---|---|---|
| `rifle.bin` | Rifle (fusils, SMG) | AssaultRifle2_1 | 33 u (84 cm) |
| `sniper.bin` | Sniper | SniperRifle_5, sans le bipied | 43 u (109 cm) |
| `pistol.bin` | Pistol | Pistol_5 | 7,6 u (19 cm) |
| `knife.bin` | Knife | Accessories/Bayonet | 12 u (30 cm) |

Grenade et bombe restent procédurales (pas d'équivalent dans le pack).
Conversion : origine à la prise de la main droite, canon vers -z, unités du jeu, couleurs
ramenées à la palette gris acier mate du moteur (une arme se lit toujours à part du corps
liquide). Changer un modèle : modifier `WEAPONS` dans `tools/weapons.py` (point de prise,
longueur), relancer avec `--preview apercu.png` pour vérifier le calage.
