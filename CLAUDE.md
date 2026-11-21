# CLAUDE.md

Ce fichier guide Claude Code (et tout contributeur) sur le projet TimeFlow.

## Le projet

TimeFlow est une horloge sur bandeau LED : un ESP32-WROOM-32 pilote un bandeau
MAX7219 de 4 modules (32x8 pixels). Il affiche `HH:MM`, l'heure vient du NTP via
le Wi-Fi, et le passage heure d'été / heure d'hiver (France) est automatique.

Le projet est un workspace Cargo de deux crates :

- `timeflow-core/` : logique indépendante du matériel (heure et fuseau, rendu,
  abstraction `Screen`, reconnexion, suivi des pannes). Il est `no_std` hors
  tests, interdit `unsafe`, ne dépend que de `log`, compile sur PC et contient
  les tests unitaires.
- `timeflow` (racine) : le firmware. `src/main.rs` (initialisation, boucle
  principale), `src/display.rs` (adaptateur MAX7219), `src/wifi.rs` (connexion
  Wi-Fi). `build.rs` lit `wifi.toml` et transmet les identifiants via `env!`.

Toute nouvelle logique qui ne touche pas directement le matériel va dans
`timeflow-core`, avec ses tests. La documentation du projet est dans `docs/`
(voir plus bas).

La cible est `xtensa-esp32-espidf` (toolchain `esp`, voir `rust-toolchain.toml`
et `.cargo/config.toml`). Le README décrit l'installation, la compilation et le
flash.

## Commandes

```bash
. ~/export-esp.sh                                          # à faire dans chaque nouveau terminal
LD_LIBRARY_PATH=$PWD/.compat-libs cargo build --release    # compiler
LD_LIBRARY_PATH=$PWD/.compat-libs cargo run --release      # compiler, flasher et ouvrir le moniteur
cargo test-core                                            # tests de timeflow-core sur PC
cargo fmt                                                  # formater
cargo fmt --check                                          # vérifier le formatage
LD_LIBRARY_PATH=$PWD/.compat-libs cargo clippy --release   # lints du firmware
cargo clippy -p timeflow-core --target x86_64-unknown-linux-gnu --all-targets  # lints du core
```

## Langues

- **Code en anglais** : identifiants (noms de types, fonctions, variables,
  modules, fichiers), messages de log et d'erreur.
- **Commentaires en français** : commentaires `//`, commentaires de
  documentation (`///`, `//!`) et commentaires de configuration (`#` dans les
  fichiers TOML). Les noms de patterns, de types et de fonctions cités dans un
  commentaire restent tels quels.
- **Tout le reste en français** : documentation dans `docs/`, README, description
  des commits, descriptions de pull request, échanges avec l'utilisateur.

## Règles de code

### Formatage

Tout le code est formaté avec `rustfmt`, suivant la configuration de
`rustfmt.toml`. Lancer `cargo fmt` avant chaque commit ; `cargo fmt --check` ne
doit signaler aucune différence.

### Commentaires obligatoires

Le code doit **obligatoirement** être commenté :

- chaque module commence par un commentaire `//!` qui explique son rôle ;
- chaque élément public (`fn`, `struct`, `enum`, `trait`, `const`, `impl`) a un
  commentaire `///` : ce qu'il fait, ses paramètres, ce qu'il renvoie, ses erreurs
  (`# Errors`) et ses cas de panique (`# Panics`) le cas échéant ;
- les fonctions privées non triviales sont aussi documentées ;
- les passages délicats (calculs de dates, manipulations de bits, contraintes
  matérielles, `unsafe`) ont un commentaire qui explique le **pourquoi**, pas
  seulement le quoi ;
- chaque bloc `unsafe` est précédé d'un commentaire `// SAFETY:` qui justifie sa
  validité.

### Bonnes pratiques Rust

- Respecter les [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
  et les conventions de nommage Rust.
- `cargo clippy` ne doit laisser aucun avertissement ; ne désactiver un lint
  (`#[allow(...)]`) qu'avec un commentaire qui explique pourquoi.
- Pas de `unwrap()` ni de `expect()` dans le code du firmware hors initialisation
  impossible à rattraper ; propager les erreurs avec `?` et `anyhow::Result`
  (application) ou des types d'erreur dédiés (bibliothèque).
- Privilégier l'emprunt au clonage, les types forts (newtypes) aux valeurs brutes,
  les `enum` aux booléens ou entiers « magiques ».
- Éviter `unsafe` ; quand il est inévitable, l'isoler dans une petite fonction
  sûre et le justifier.
- Garder la logique pure dans `timeflow-core`, séparée du code matériel, pour
  pouvoir la tester ; chaque logique nouvelle vient avec ses tests unitaires.
- Pas d'allocation inutile dans la boucle d'affichage : le firmware tourne sur un
  microcontrôleur aux ressources limitées.

### Découplage

Favoriser le découplage entre les composants :

- dépendre d'abstractions (traits) plutôt que de types concrets, en particulier
  pour le matériel (affichage, source d'heure, réseau) ;
- injecter les dépendances (paramètres, génériques) plutôt que de les créer au
  cœur de la logique ;
- un module = une responsabilité ; limiter ce qui est `pub` au strict nécessaire ;
- la logique métier ne doit jamais dépendre directement d'`esp-idf-svc` ou du
  pilote `max7219` : seul le firmware (`src/`) les connaît.

### Design patterns

Favoriser l'utilisation des design patterns quand ils simplifient le code ou
renforcent le découplage (par exemple Strategy, Adapter, State, Builder, Observer,
Newtype, RAII, Typestate). Ne pas en ajouter s'ils n'apportent rien.

Chaque pattern utilisé doit être **documenté dans `docs/`** avec :

- le **nom du pattern** ;
- l'endroit où il est utilisé (fichier, type, fonction) ;
- le problème qu'il résout et pourquoi il a été choisi ;
- un diagramme Mermaid si cela aide à comprendre.

Le fichier `docs/design-patterns.md` recense tous les patterns utilisés dans le
projet : le mettre à jour à chaque ajout, modification ou retrait d'un pattern.
Un commentaire dans le code peut aussi nommer le pattern (par exemple
`// Pattern Adapter : expose le pilote MAX7219 derrière le trait Screen`).

## Documentation

- Toute la documentation du projet va dans le dossier `docs/`, en français.
- Le README reste le point d'entrée (présentation, câblage, installation) et
  renvoie vers `docs/` pour le reste.
- **Tous les diagrammes sont faits en Mermaid** (blocs ```` ```mermaid ````),
  jamais en image ni en art ASCII.
- Mettre la documentation à jour dans le même commit que le code qu'elle décrit.

## Revue de code avant chaque commit

Aucun commit sans revue de code préalable :

1. `cargo fmt --check` et `cargo clippy` (firmware et core) passent sans erreur
   ni avertissement ;
2. les tests passent (`cargo test-core`) et le firmware compile
   (`cargo build --release`) ;
3. une revue du diff est faite, par l'agent `rust-developer-senior` ou avec
   `/code-review`, et par l'agent `software-architect` quand le changement touche
   l'architecture (nouveau module, nouveau trait, nouveau pattern, nouvelle
   dépendance) ;
4. les remarques de la revue sont corrigées, ou explicitement écartées avec
   l'accord de l'utilisateur ;
5. ensuite seulement, on commite.

## Commits : Conventional Commits

Les messages de commit suivent la spécification
[Conventional Commits 1.0.0](https://www.conventionalcommits.org/fr/v1.0.0/) :

```
<type>(<portée optionnelle>): <description en français>

<corps optionnel, en français>

<pied optionnel>
```

- Types : `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`,
  `ci`, `chore`, `revert`.
- Portées usuelles : `clock`, `display`, `wifi`, `ntp`, `build`, `docs`.
- Description à l'impératif, en minuscule, sans point final, 72 caractères au plus
  pour la première ligne.
- Changement incompatible : `!` après le type (`feat!: ...`) et un pied
  `BREAKING CHANGE: ...`.

Exemples :

```
feat(clock): ajoute l'affichage des secondes
fix(wifi): relance la connexion après une perte de signal
docs: documente le pattern Adapter de l'affichage
```

## Agents

Deux agents spécialisés sont définis dans `.claude/agents/` :

- `rust-developer-senior` : implémentation, revue de code Rust et embarqué ESP32 ;
- `software-architect` : conception, découplage, choix et documentation des
  design patterns, diagrammes Mermaid.
