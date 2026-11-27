# Branches, PR et releases

Modèle trunk-based : une seule branche permanente, `main`, toujours livrable. Pas de branche `dev`.

## Protection de `main` (ruleset GitHub)

- Aucun push direct, aucune suppression, aucun force push.
- PR obligatoire avec 1 approbation ; les admins de l'org peuvent passer outre, dans une PR seulement.
- Checks requis : `lint`, `test`, `docker` (CI) et `pr-title` (titre Conventional Commits).
- Fusion en **squash** uniquement, branche supprimée après fusion.

## Cycle

1. `git switch main && git pull && git switch -c fix/probleme` (préfixes `feat/`, `fix/`, `docs/`, `chore/`, `ci/`, `refactor/`, `perf/`, `test/`).
2. PR vers `main`, titre au format [Conventional Commits](https://www.conventionalcommits.org/fr/) : `fix(server): …`, `feat(client): …`. Le squash fait du titre le commit lu par release-please.
3. Revue, CI verte, fusion.
4. release-please ouvre ou met à jour la PR `chore(main): release x.y.z` : elle cumule tout ce qui a été fusionné depuis la dernière version (changelog, `version.txt`, `Cargo.toml`, `Cargo.lock`).
5. Fusionner cette PR quand on veut publier : tag `vx.y.z`, pre-release GitHub, puis `release.yml` y attache les archives Windows et Linux.

Types qui déclenchent une version : `feat` (mineure), `fix` et `perf` (patch), `!` ou `BREAKING CHANGE:` (majeure). `docs`, `chore`, `ci`, `test`, `refactor`, `style`, `build` n'en déclenchent pas.

Versions alpha (`versioning: prerelease`, `prerelease-type: alpha`) : pour passer en stable, fusionner une PR dont le corps du commit contient `Release-As: 0.1.0`.

Corriger une ancienne version : créer alors une branche `release/0.x` depuis son tag et y recopier le correctif (cherry-pick). Pas avant d'en avoir besoin.

## GitHub App de release

release-please utilise une GitHub App de l'org plutôt que `GITHUB_TOKEN`, sinon sa PR et son tag ne déclenchent aucun workflow (CI jamais lancée, archives jamais construites).

- Permissions du repo : Contents et Pull requests en lecture/écriture, Metadata en lecture ; webhook désactivé ; installée sur ce repo seulement.
- Org : variable `RELEASE_APP_ID`, secret `RELEASE_APP_PRIVATE_KEY` (clé `.pem` de l'App).

## Identité

Commits au nom du noreply GitHub du compte (`<id>+<login>@users.noreply.github.com`), jamais d'adresse professionnelle ou d'école.
