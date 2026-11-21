//! Logique pure (sans dépendance ESP32) : fuseau horaire et rendu de l'image.
//! Testable sur PC.

/// Nombre de modules 8x8 chaînés (bandeau 32x8).
pub const MODULES: usize = 4;

/// Image complète : 8 lignes de 32 pixels, bit 31 = colonne de gauche.
pub type Frame = [u32; 8];

/// Police 5x7 : 7 lignes par chiffre, bit 4 = pixel de gauche.
pub const FONT: [[u8; 7]; 10] = [
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

/// Tiret (pendant la connexion) : une barre au milieu.
const DASH: [u8; 7] = [0, 0, 0, 0b11111, 0, 0, 0];

/// Colonne de gauche de chaque chiffre de "HH:MM", et des deux points.
const DIGIT_X: [u32; 4] = [2, 8, 19, 25];
const COLON_X: u32 = 15;

/// Options d'affichage.
pub struct Style {
    /// Deux points qui clignotent chaque seconde.
    pub blink_colon: bool,
    /// Rotation de chaque module, en quarts de tour horaire (0 à 3), selon le montage.
    pub rotation: u8,
    /// Inverse l'ordre des modules (si les chiffres sortent dans le mauvais ordre).
    pub reverse_modules: bool,
}

/// Nombre de jours depuis le 1970-01-01 pour une date du calendrier grégorien.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Année correspondant à un nombre de jours depuis le 1970-01-01.
fn year_from_days(z: i64) -> i64 {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    yoe + era * 400 + if m <= 2 { 1 } else { 0 }
}

/// Jour (en jours depuis 1970) du dernier dimanche d'un mois de 31 jours.
fn last_sunday(year: i64, month: i64) -> i64 {
    let d31 = days_from_civil(year, month, 31);
    // 1970-01-01 était un jeudi : (d + 4) % 7 == 0 correspond au dimanche
    d31 - (d31 + 4).rem_euclid(7)
}

/// Heure de Paris (CET/CEST, changement d'heure européen) à partir de l'heure UTC.
/// Renvoie (heures, minutes, secondes).
pub fn paris_time(utc_secs: u64) -> (u8, u8, u8) {
    let utc = utc_secs as i64;
    let year = year_from_days(utc.div_euclid(86_400));
    // Heure d'été : du dernier dimanche de mars au dernier dimanche d'octobre, à 01:00 UTC
    let dst_start = last_sunday(year, 3) * 86_400 + 3_600;
    let dst_end = last_sunday(year, 10) * 86_400 + 3_600;
    let offset = if (dst_start..dst_end).contains(&utc) {
        7_200
    } else {
        3_600
    };
    let local = (utc + offset).rem_euclid(86_400);
    (
        (local / 3_600) as u8,
        (local / 60 % 60) as u8,
        (local % 60) as u8,
    )
}

/// Dessine un glyphe 5x7 avec sa colonne de gauche en `x` (lignes 0 à 6).
fn draw_glyph(frame: &mut Frame, glyph: &[u8; 7], x: u32) {
    for (y, bits) in glyph.iter().enumerate() {
        frame[y] |= (*bits as u32) << (27 - x);
    }
}

/// Dessine les deux points (2 pixels de large) en `COLON_X`.
fn draw_colon(frame: &mut Frame) {
    for y in [1, 2, 4, 5] {
        frame[y] |= 0b11 << (30 - COLON_X);
    }
}

/// Image de l'heure "HH:MM".
pub fn render(h: u8, m: u8, s: u8, style: &Style) -> Frame {
    let mut frame = [0; 8];
    for (digit, x) in [h / 10, h % 10, m / 10, m % 10].iter().zip(DIGIT_X) {
        draw_glyph(&mut frame, &FONT[*digit as usize], x);
    }
    if !style.blink_colon || s % 2 == 0 {
        draw_colon(&mut frame);
    }
    frame
}

/// Image affichée pendant la connexion : "--:--", les deux points clignotent.
pub fn waiting_pattern(step: u32) -> Frame {
    let mut frame = [0; 8];
    for x in DIGIT_X {
        draw_glyph(&mut frame, &DASH, x);
    }
    if step % 2 == 0 {
        draw_colon(&mut frame);
    }
    frame
}

/// Tourne un bloc 8x8 d'un quart de tour dans le sens horaire
/// (`block[ligne]`, bit 7 = colonne de gauche).
fn rotate_cw(block: [u8; 8]) -> [u8; 8] {
    let mut out = [0u8; 8];
    for (r, row) in out.iter_mut().enumerate() {
        for c in 0..8 {
            // Le pixel (r, c) de l'image tournée vient du pixel (7 - c, r) de l'original
            if (block[7 - c] >> (7 - r)) & 1 == 1 {
                *row |= 1 << (7 - c);
            }
        }
    }
    out
}

/// Découpe l'image en données par module, indexées par adresse dans la chaîne
/// (adresse 0 = dernier module de la chaîne, comme dans la crate `max7219`).
pub fn to_modules(frame: &Frame, style: &Style) -> [[u8; 8]; MODULES] {
    let mut modules = [[0u8; 8]; MODULES];
    for i in 0..MODULES {
        let mut block = [0u8; 8];
        for (r, row) in block.iter_mut().enumerate() {
            *row = (frame[r] >> (8 * (MODULES - 1 - i))) as u8;
        }
        for _ in 0..style.rotation % 4 {
            block = rotate_cw(block);
        }
        let addr = if style.reverse_modules {
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

    const PLAIN: Style = Style {
        blink_colon: false,
        rotation: 0,
        reverse_modules: false,
    };

    /// Pixel (x, y) allumé ?
    fn px(frame: &Frame, x: u32, y: usize) -> bool {
        (frame[y] >> (31 - x)) & 1 == 1
    }

    #[test]
    fn winter_time_is_utc_plus_1() {
        // 2026-11-17 12:34:56 UTC
        assert_eq!(paris_time(1_794_918_896), (13, 34, 56));
    }

    #[test]
    fn summer_time_is_utc_plus_2() {
        // 2026-07-14 10:00:00 UTC
        assert_eq!(paris_time(1_784_023_200), (12, 0, 0));
    }

    #[test]
    fn dst_switches_on_last_sunday_of_march() {
        // 2026-03-29 00:59:59 UTC -> 01:59:59, puis 01:00:00 UTC -> 03:00:00
        assert_eq!(paris_time(1_774_745_999), (1, 59, 59));
        assert_eq!(paris_time(1_774_746_000), (3, 0, 0));
    }

    #[test]
    fn dst_ends_on_last_sunday_of_october() {
        // 2026-10-25 00:59:59 UTC -> 02:59:59, puis 01:00:00 UTC -> 02:00:00
        assert_eq!(paris_time(1_792_889_999), (2, 59, 59));
        assert_eq!(paris_time(1_792_890_000), (2, 0, 0));
    }

    #[test]
    fn render_places_digits_and_colon() {
        let frame = render(12, 34, 0, &PLAIN);
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
    fn colon_blinks() {
        let style = Style {
            blink_colon: true,
            ..PLAIN
        };
        assert!(px(&render(12, 34, 0, &style), 15, 1));
        assert!(!px(&render(12, 34, 1, &style), 15, 1));
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
        let m = to_modules(&frame, &PLAIN);
        assert_eq!(m[0][0], 0x80); // module de gauche
        assert_eq!(m[3][0], 0x01); // module de droite
        let rev = Style {
            reverse_modules: true,
            ..PLAIN
        };
        let m = to_modules(&frame, &rev);
        assert_eq!(m[3][0], 0x80);
        assert_eq!(m[0][0], 0x01);
    }
}
