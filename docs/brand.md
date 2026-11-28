# Identité visuelle

Fichiers dans [`assets/brand/`](../assets/brand/) :

| Fichier | Usage |
|---|---|
| `logo.svg` | Monogramme seul (fond transparent) |
| `logo.png` | `logo.svg` rastérisé en 432×512 (fond transparent), embarqué par le client pour l'écran d'accueil |
| `icon.svg` | Icône 512 px et plus petite jusqu'à 64 px |
| `icon-small.svg` | Favicon 32 et 16 px : S plein, sans parois |
| `banner.png` | En-tête du README (2560×720) |
| `social-preview.png` | Aperçu GitHub, Settings → General → Social preview (1280×640) |

## Concept

Le S est un récipient : un tube creux taillé à 45°, comme les personnages low-poly
([personnages](https://github.com/SforSkill/Project-s-engine/blob/main/docs/characters.md)), à moitié rempli de leur peinture liquide. La
surface du liquide est elle-même facettée. Le contenant est exact, le contenu est vivant.

## Couleurs

| Nom | Hex | Rôle | Contraste sur Graphite |
|---|---|---|---|
| Graphite | `#121417` | fond | — |
| Craie | `#ECE8E1` | texte, parois du logo | 15:1 |
| Améthyste | `#9B6BFF` | marque, liquide (clair `#B796FF`, sombre `#7446E0`) | 5,2:1 |
| Minium | `#F0612A` | camp attaquant (en jeu) | 6,4:1 |
| Cobalt | `#4C7DFF` | camp défenseur (en jeu) | 5,0:1 |
| Acier | `#8A9099` | données, texte secondaire | 5,6:1 |

## Polices

- Titres : Archivo Expanded 800 (`font-stretch: 125%`), en capitales.
- Texte : Archivo 400.
- Données (ticks, versions, latences) : JetBrains Mono.

Toutes deux sous licence OFL.

## Règles

- Un seul niveau de liquide ; facettes à 45° ; une règle de ticks (128 Hz) comme seul ornement ; aplats mats.
- Pas de néons, halos, dégradés, éclaboussures ni réticule dans le logo.
- Sous 64 px, utiliser `icon-small.svg` : les parois deviennent illisibles.

## Dans le client

`crates/ps-client/src/theme.rs` applique cette charte à egui : fond Graphite, texte Craie,
accent Améthyste, données en Acier, angles droits, aucune ombre. Les polices sont dans
`assets/fonts/` (OFL). L'écran d'accueil a un rail à gauche (monogramme, titre, règle de ticks,
navigation) ; l'action principale est un bouton Améthyste aux coins coupés à 45°.

Les textures de la carte (`assets/textures/`, sources CC0 dans `SOURCES.md`) suivent la même
règle d'aplats mats : luminance seule, postérisée, faible contraste, teinte donnée par le matériau.
Les armes (`assets/weapons/`, CC0 Quaternius) sont des modèles low-poly en couleurs unies,
ramenées à la palette gris acier mate du moteur ; le ciel (`assets/sky/`, CC0 Poly Haven) est
adouci et désaturé pour rester calme derrière les aplats.

Régénérer `logo.png` après un changement du SVG : l'ouvrir à 432×512 dans un navigateur
sans fond (par exemple Edge `--headless --default-background-color=00000000 --screenshot`).

La maquette source (canevas de conception) n'est pas versionnée : régénérer les PNG
depuis le SVG et ces règles si l'identité change.
