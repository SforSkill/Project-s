# Projet S — démarrage rapide

## Jouer sous Windows

1. Décompresser `Project-S-Windows-x64.zip`.
2. Lancer `Project-S\Project-S.exe`.
3. Menu **Jouer** : choisir la carte, le mode et le nombre de bots, puis
   **Lancer la partie**. Un serveur 128 Hz démarre en local, dans le jeu.

Pré-requis : Windows 10/11 64 bits, carte graphique compatible Vulkan ou DirectX 12.

Si Windows SmartScreen bloque l'exécutable (il n'est pas signé dans l'alpha) :
**Informations complémentaires → Exécuter quand même**.

## Jouer sous Linux

```sh
tar xzf Project-S-Linux-x64.tar.gz
./Project-S/Project-S
```

Le build Linux du client est produit par la CI, mais il n'a pas été testé sur
une machine Linux avec écran (voir `docs/validation-report.md`).

## Rejoindre un serveur

Menu **Rejoindre** : saisir `adresse:port` (port par défaut : 27015/UDP), puis
**Se connecter**. En ligne de commande : `Project-S.exe --connect 203.0.113.10:27015`.

## Héberger un serveur dédié (Linux)

```sh
tar xzf Project-S-Dedicated-Server-Linux-x64.tar.gz
cd projet-s-server
./projet-s-server --map ps_citadel --mode competitive --fill-bots 1
```

- Ouvrir le port **27015/UDP** dans le pare-feu.
- L'API d'administration écoute sur `127.0.0.1:27016`.
- Toutes les options : `./projet-s-server --help`.
- Docker, systemd et Kubernetes : `docs/deployment.md`.

## Commandes par défaut

Ce sont des touches **physiques** : sur un clavier AZERTY, avancer se trouve sur Z
et aller à gauche sur Q.

| Action | Touche |
|---|---|
| Se déplacer | W A S D (Z Q S D en AZERTY) |
| Sauter / s'accroupir / marcher | Espace / Ctrl gauche / Maj gauche |
| Tirer / viser (lunette) | clic gauche / clic droit |
| Armes | 1 à 5 ; Q (A en AZERTY) = arme précédente |
| Recharger / utiliser (poser, désamorcer) / lâcher | R / E / G |
| Acheter | B (en zone d'achat, pendant le temps d'achat) |
| Scores | Tab |
| Inspecteur de tirs | F3 |
| Discussion (tous / équipe) | Y / U |
| Menu (équipe, paramètres, quitter) | Échap |
| Plein écran / fenêtré | F11 ou Alt+Entrée |

En partie, le curseur est masqué et retenu dans la fenêtre. Il est libéré quand
un menu est ouvert (Échap, achat, discussion) ou quand la fenêtre perd le focus
(Alt+Tab), puis il est capturé de nouveau au retour.

## Paramètres

Les paramètres sont dans **Échap → Paramètres** :
- sensibilité, avec m_yaw et m_pitch à 0,022 comme dans les jeux Source (une même valeur donne le même cm/360°) ;
- FOV ;
- viewmodel ;
- réticule ;
- volume ;
- limite d'images, V-Sync, plein écran ;
- graphe réseau et variante réseau.

Ils sont enregistrés dans `%APPDATA%\ProjetS\settings.toml` sous Windows, et dans
`~/.config/ProjetS/settings.toml` sous Linux. Les touches se changent dans la
section `[keys]` de ce fichier (pas encore depuis le menu).

## Modes

| Profil | Contenu |
|---|---|
| `competitive` | 5v5, MR15 (30 manches), bombe, économie, prolongations MR3 |
| `competitive_mr12` / `competitive_mr16` | mêmes règles, 24 ou 32 manches |
| `duel` | 1v1 en manches courtes |
| `deathmatch` | réapparition, pas d'économie |
| `training` | carte d'entraînement, cibles, 16 000 $ |

## Problèmes connus

Voir la section « Limites » de `docs/status.md`.
