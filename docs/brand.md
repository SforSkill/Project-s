# Identité visuelle

Fichiers dans [`assets/brand/`](../assets/brand/) :

| Fichier | Usage |
|---|---|
| `logo.svg` | Monogramme seul (fond transparent) |
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

La maquette source (canevas de conception) n'est pas versionnée : régénérer les PNG
depuis le SVG et ces règles si l'identité change.
