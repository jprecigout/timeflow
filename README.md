<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/timeflow-logo-sombre.svg">
    <source media="(prefers-color-scheme: light)" srcset="assets/timeflow-logo-clair.svg">
    <img src="assets/timeflow-logo-clair.svg" alt="Logo TimeFlow">
  </picture>
</p>

# TimeFlow

Horloge sur bandeau LED (ESP32-WROOM-32 + MAX7219 4 modules).

Affiche l'heure `HH:MM` en chiffres 5x7, sans défilement, sur un bandeau
MAX7219 « 4 en 1 » (4 modules 8x8 = 32x8 pixels). Les deux points clignotent
chaque seconde. Heure récupérée par NTP via le Wi-Fi, avec passage automatique
heure d'été / heure d'hiver (France).

```
....#....###.......#####....#...
...##...#...#..##.....#....##...
....#.......#..##....#....#.#...
....#......#..........#..#..#...
....#.....#....##......#.#####..
....#....#.....##..#...#....#...
...###..#####.......###.....#...
```

## Câblage

Sur le connecteur d'entrée du bandeau (côté `DIN`) :

| MAX7219 | ESP32    |
|---------|----------|
| VCC     | 5V (VIN) |
| GND     | GND      |
| DIN     | GPIO23   |
| CS      | GPIO5    |
| CLK     | GPIO18   |

## Si l'affichage est tourné ou dans le désordre

Selon le modèle de bandeau, les modules sont montés différemment. Dans
`src/main.rs`, ajuster `STYLE` puis reflasher :

- chiffres tournés ou en miroir dans chaque module : changer `rotation` (0, 1, 2 ou 3) ;
- morceaux de chiffres dans le mauvais ordre : passer `reverse_modules` à `true`.

## Installation (une seule fois)

```bash
cargo install espup espflash ldproxy --locked
espup install --targets esp32
```

Puis, dans chaque nouveau terminal :

```bash
. ~/export-esp.sh
```

### Ubuntu 26.10 (ou distribution sans `libxml2.so.2`)

Le clang installé par ESP-IDF cherche `libxml2.so.2`, qui n'existe plus.
Un lien local dans le projet suffit (rien n'est modifié sur le système).
Il est déjà présent dans `.compat-libs/` ; pour le recréer :

```bash
mkdir -p .compat-libs && ln -sf /usr/lib/x86_64-linux-gnu/libxml2.so.16 .compat-libs/libxml2.so.2
```

## Configuration

Les identifiants Wi-Fi sont dans `wifi.toml` (non versionné). Pour le créer :

```bash
cp wifi.toml.example wifi.toml
```

puis y renseigner `ssid` et `password`. Ils sont intégrés au firmware à la
compilation : recompiler et reflasher après chaque modification.

Dans `src/main.rs`, on peut aussi ajuster `INTENSITY` (luminosité, de 0x00 à 0x0F)
et `STYLE` (clignotement des deux points, orientation des modules).

Sur le réseau, l'ESP32 s'annonce sous le nom `timeflow` (réglé par
`CONFIG_LWIP_LOCAL_HOSTNAME` dans `sdkconfig.defaults`).

## Compiler et flasher

Brancher l'ESP32 en USB, puis :

```bash
. ~/export-esp.sh
LD_LIBRARY_PATH=$PWD/.compat-libs cargo run --release
```

Pour compiler seulement, sans flasher :

```bash
LD_LIBRARY_PATH=$PWD/.compat-libs cargo build --release
```

La première compilation télécharge ESP-IDF (plusieurs Go dans `.embuild/`)
et prend une dizaine de minutes.

## Formatage du code

Le style est défini dans `rustfmt.toml` (rustfmt est fourni avec la toolchain `esp`) :

```bash
cargo fmt          # formate le code
cargo fmt --check  # vérifie seulement, sans rien modifier
```

## Licence

Ce projet est distribué sous la licence Apache 2.0. Voir le fichier [LICENSE](LICENSE).
