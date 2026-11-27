## Quoi et pourquoi

## Périmètre

- [ ] Le changement reste dans le périmètre de `SCOPE.md`
- [ ] Contrat modifié : version et changelog mis à jour, dépôts consommateurs listés

## Vérifié

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- [ ] `cargo test --workspace --all-features --release --locked` (Project-s-content voisin)
- [ ] `Cargo.lock` produit sans `.cargo/config.toml` local (aucun patch)
