//! Rendu du cadran dans une image 32x8, et conversion de cette image en
//! données brutes pour chaque module MAX7219 8x8.

use crate::time::Time;

/// Nombre de modules 8x8 chaînés (bandeau 32x8).
pub const MODULES: usize = 4;

/// Image complète : 8 lignes de 32 pixels, le bit 31 est la colonne de gauche.
pub type Frame = [u32; 8];

/// Données brutes d'un module 8x8 : `block[ligne]`, le bit 7 est la colonne de gauche.
pub type ModuleBlock = [u8; 8];

/// Police 5x7 : 7 lignes par chiffre, le bit 4 est le pixel de gauche.
const FONT: [[u8; 7]; 10] = [
    [
        0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
    ], // 0
    [
        0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
    ], // 1
    [
        0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
    ], // 2
    [
        0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110,
    ], // 3
    [
        0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
    ], // 4
    [
        0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
    ], // 5
    [
        0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
    ], // 6
    [
        0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
    ], // 7
    [
        0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
    ], // 8
    [
        0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
    ], // 9
];

/// Tiret affiché pendant l'attente du réseau : une barre sur la ligne du milieu.
const DASH: [u8; 7] = [0, 0, 0, 0b11111, 0, 0, 0];

/// Colonne de gauche de chaque chiffre de "HH:MM".
const DIGIT_X: [u32; 4] = [2, 8, 19, 25];
/// Colonne de gauche des deux points (2 pixels de large).
const COLON_X: u32 = 15;

/// Options d'affichage du cadran, utilisées par `render`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    /// Deux points qui clignotent chaque seconde.
    pub blink_colon: bool,
}

/// Rotation d'un module, dans le sens horaire.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rotation {
    /// Pas de rotation.
    #[default]
    None,
    /// Un quart de tour.
    Quarter,
    /// Un demi-tour.
    Half,
    /// Trois quarts de tour.
    ThreeQuarters,
}

/// Conversion en nombre de quarts de tour.
impl Rotation {
    /// Nombre de quarts de tour horaire (0 à 3).
    pub const fn quarter_turns(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Quarter => 1,
            Self::Half => 2,
            Self::ThreeQuarters => 3,
        }
    }
}

/// Montage physique des modules sur le bandeau, utilisé par `to_modules`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModuleLayout {
    /// Rotation de chaque module, selon son montage sur le bandeau.
    pub rotation: Rotation,
    /// Inverse l'ordre des modules (quand les chiffres sortent dans le
    /// désordre).
    pub reverse_modules: bool,
}

/// Dessine un glyphe 5x7 avec sa colonne de gauche en `x` (lignes 0 à 6).
fn draw_glyph(frame: &mut Frame, glyph: &[u8; 7], x: u32) {
    for (row, bits) in frame.iter_mut().zip(glyph) {
        // Le bit 4 du glyphe doit arriver sur le bit 31 - x de la ligne.
        *row |= u32::from(*bits) << (27 - x);
    }
}

/// Dessine les deux points (2 pixels de large) en `COLON_X`.
fn draw_colon(frame: &mut Frame) {
    for y in [1, 2, 4, 5] {
        frame[y] |= 0b11 << (30 - COLON_X);
    }
}

/// Dessine l'heure au format "HH:MM". Avec `style.blink_colon`, les deux points
/// ne sont affichés que les secondes paires.
pub fn render(time: Time, style: &Style) -> Frame {
    let mut frame = [0; 8];
    let (h, m) = (time.hours(), time.minutes());
    // `Time` garantit h < 24 et m < 60 : chaque chiffre est un index valide de la police.
    for (digit, x) in [h / 10, h % 10, m / 10, m % 10].into_iter().zip(DIGIT_X) {
        draw_glyph(&mut frame, &FONT[usize::from(digit)], x);
    }
    if !style.blink_colon || time.seconds() % 2 == 0 {
        draw_colon(&mut frame);
    }
    frame
}

/// Image affichée pendant l'attente du réseau ou de l'heure : "--:--", avec les
/// deux points qui clignotent à chaque `step`.
pub fn waiting_pattern(step: u64) -> Frame {
    let mut frame = [0; 8];
    for x in DIGIT_X {
        draw_glyph(&mut frame, &DASH, x);
    }
    if step % 2 == 0 {
        draw_colon(&mut frame);
    }
    frame
}

/// Tourne un bloc 8x8 d'un quart de tour dans le sens horaire.
fn rotate_cw(block: ModuleBlock) -> ModuleBlock {
    let mut out = [0u8; 8];
    for (r, row) in out.iter_mut().enumerate() {
        for c in 0..8 {
            // Le pixel (r, c) du bloc tourné vient du pixel (7 - c, r) de l'original.
            if (block[7 - c] >> (7 - r)) & 1 == 1 {
                *row |= 1 << (7 - c);
            }
        }
    }
    out
}

/// Découpe l'image en données par module, indexées par adresse dans la chaîne.
///
/// Sans `layout.reverse_modules`, l'adresse 0 reçoit les 8 colonnes de gauche.
/// Chaque bloc est tourné selon `layout.rotation`.
pub fn to_modules(frame: &Frame, layout: &ModuleLayout) -> [ModuleBlock; MODULES] {
    let mut modules = [[0u8; 8]; MODULES];
    for i in 0..MODULES {
        let mut block = [0u8; 8];
        for (row, line) in block.iter_mut().zip(frame) {
            // Troncature voulue : on ne garde que les 8 colonnes du module i.
            *row = (line >> (8 * (MODULES - 1 - i))) as u8;
        }
        for _ in 0..layout.rotation.quarter_turns() {
            block = rotate_cw(block);
        }
        let addr = if layout.reverse_modules {
            MODULES - 1 - i
        } else {
            i
        };
        modules[addr] = block;
    }
    modules
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Style = Style { blink_colon: false };
    const STRAIGHT: ModuleLayout = ModuleLayout {
        rotation: Rotation::None,
        reverse_modules: false,
    };

    /// Le pixel (x, y) est-il allumé ?
    fn px(frame: &Frame, x: u32, y: usize) -> bool {
        (frame[y] >> (31 - x)) & 1 == 1
    }

    /// Raccourci pour une heure valide dans les tests.
    fn time(h: u8, m: u8, s: u8) -> Time {
        Time::new(h, m, s).expect("valid time")
    }

    #[test]
    fn render_places_digits_and_colon() {
        let frame = render(time(12, 34, 0), &PLAIN);
        // "1" : barre verticale en colonne 2 + 2 = 4
        assert!((0..7).all(|y| px(&frame, 4, y)));
        // "2" : ligne du bas pleine (colonnes 8 à 12)
        assert!((8..13).all(|x| px(&frame, x, 6)));
        // deux points en colonnes 15-16, lignes 1, 2, 4, 5
        assert!(px(&frame, 15, 1) && px(&frame, 16, 5) && !px(&frame, 15, 3));
        // ligne 7 et marges vides
        assert_eq!(frame[7], 0);
        assert!((0..8).all(|y| !px(&frame, 0, y) && !px(&frame, 31, y)));
    }

    #[test]
    fn render_accepts_the_largest_time() {
        let frame = render(time(23, 59, 59), &PLAIN);
        assert_ne!(frame, [0; 8]);
    }

    #[test]
    fn colon_blinks() {
        let style = Style { blink_colon: true };
        assert!(px(&render(time(12, 34, 0), &style), 15, 1));
        assert!(!px(&render(time(12, 34, 1), &style), 15, 1));
    }

    #[test]
    fn waiting_pattern_blinks_colon() {
        assert!(px(&waiting_pattern(0), 15, 1));
        assert!(!px(&waiting_pattern(1), 15, 1));
        // tirets sur la ligne du milieu
        assert!(px(&waiting_pattern(1), 2, 3));
    }

    #[test]
    fn four_rotations_are_identity() {
        let block = [0x81, 0x42, 0x24, 0x18, 0xF0, 0x0F, 0x01, 0x80];
        let mut b = block;
        for _ in 0..4 {
            b = rotate_cw(b);
        }
        assert_eq!(b, block);
    }

    #[test]
    fn rotate_moves_top_left_to_top_right() {
        let mut block = [0u8; 8];
        block[0] = 0x80; // pixel en haut à gauche
        let r = rotate_cw(block);
        assert_eq!(r[0], 0x01); // devient en haut à droite
    }

    #[test]
    fn modules_split_and_order() {
        let mut frame = [0u32; 8];
        frame[0] = 0x8000_0001; // pixel tout à gauche et tout à droite
        let m = to_modules(&frame, &STRAIGHT);
        assert_eq!(m[0][0], 0x80); // module de gauche
        assert_eq!(m[3][0], 0x01); // module de droite
        let rev = ModuleLayout {
            reverse_modules: true,
            ..STRAIGHT
        };
        let m = to_modules(&frame, &rev);
        assert_eq!(m[3][0], 0x80);
        assert_eq!(m[0][0], 0x01);
    }

    #[test]
    fn layout_rotation_is_applied() {
        let mut frame = [0u32; 8];
        frame[0] = 0x8000_0000; // pixel en haut à gauche du module de gauche
        let quarter = ModuleLayout {
            rotation: Rotation::Quarter,
            ..STRAIGHT
        };
        // Un quart de tour horaire l'amène en haut à droite du module.
        assert_eq!(to_modules(&frame, &quarter)[0][0], 0x01);
    }
}
