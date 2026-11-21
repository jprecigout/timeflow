//! Heure du jour et conversion de fuseau horaire.
//!
//! La conversion de l'heure UTC en heure locale est calculée à la main plutôt
//! qu'avec le support `TZ` POSIX d'ESP-IDF, pour rester pure et testable sur PC
//! (voir `docs/adr/0001-fuseau-horaire-en-rust-pur.md`).

/// Nombre de secondes dans une heure.
const SECS_PER_HOUR: i64 = 3_600;
/// Nombre de secondes dans une journée.
const SECS_PER_DAY: i64 = 86_400;
/// Dernier instant pris en charge (9999-12-31 23:59:59 UTC). Les horodatages
/// plus grands sont ramenés à cette valeur, pour que les calculs de calendrier
/// ne puissent pas déborder.
const MAX_UTC_SECS: u64 = 253_402_300_799;

/// Heure du jour validée : heures dans `0..24`, minutes et secondes dans `0..60`.
///
/// Pattern Newtype : les champs sont privés et chaque constructeur vérifie les
/// bornes. Une heure invalide ne peut donc pas exister, et le rendu peut indexer
/// la police sans risque de panique.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Time {
    hours: u8,
    minutes: u8,
    seconds: u8,
}

/// Construction et lecture d'une heure du jour.
impl Time {
    /// Construit une heure du jour, ou renvoie `None` si un champ est hors bornes.
    pub fn new(hours: u8, minutes: u8, seconds: u8) -> Option<Self> {
        (hours < 24 && minutes < 60 && seconds < 60).then_some(Self {
            hours,
            minutes,
            seconds,
        })
    }

    /// Convertit un horodatage UNIX (secondes depuis le 1970-01-01 UTC) en heure
    /// locale dans le fuseau donné. Les horodatages au-delà de l'an 9999 sont
    /// ramenés au 9999-12-31 23:59:59 UTC.
    pub fn from_utc(utc_secs: u64, zone: &impl TimeZone) -> Self {
        // MAX_UTC_SECS tient dans un i64 : la conversion ne peut pas échouer.
        let utc = utc_secs.min(MAX_UTC_SECS) as i64;
        let local = utc + zone.utc_offset_secs(utc);
        // rem_euclid garde le résultat dans 0..86_400 : les conversions
        // ci-dessous ne tronquent rien.
        let secs_of_day = local.rem_euclid(SECS_PER_DAY);
        Self {
            hours: (secs_of_day / SECS_PER_HOUR) as u8,
            minutes: (secs_of_day / 60 % 60) as u8,
            seconds: (secs_of_day % 60) as u8,
        }
    }

    /// Heures, dans `0..24`.
    pub fn hours(self) -> u8 {
        self.hours
    }

    /// Minutes, dans `0..60`.
    pub fn minutes(self) -> u8 {
        self.minutes
    }

    /// Secondes, dans `0..60`.
    pub fn seconds(self) -> u8 {
        self.seconds
    }
}

/// Fuseau horaire : donne le décalage par rapport à UTC en vigueur à un instant.
///
/// Pattern Strategy : `Time::from_utc` fonctionne avec n'importe quelle
/// implémentation. On peut ajouter un autre pays ou un décalage fixe sans
/// toucher à la conversion ni au rendu.
pub trait TimeZone {
    /// Décalage par rapport à UTC, en secondes, en vigueur à `utc_secs` (horodatage UNIX).
    fn utc_offset_secs(&self, utc_secs: i64) -> i64;
}

/// Fuseau de Paris : CET (UTC+1) en hiver, CEST (UTC+2) en été, selon la règle
/// de l'Union européenne (du dernier dimanche de mars au dernier dimanche
/// d'octobre, changement à 01:00 UTC).
#[derive(Clone, Copy, Debug, Default)]
pub struct EuropeParis;

/// Règle européenne : UTC+2 entre les deux changements d'heure, UTC+1 sinon.
impl TimeZone for EuropeParis {
    fn utc_offset_secs(&self, utc_secs: i64) -> i64 {
        // Les deux changements d'heure tombent en milieu d'année : l'année UTC
        // est donc toujours la bonne pour les calculer.
        let year = year_from_days(utc_secs.div_euclid(SECS_PER_DAY));
        let dst_start = last_sunday(year, 3) * SECS_PER_DAY + SECS_PER_HOUR;
        let dst_end = last_sunday(year, 10) * SECS_PER_DAY + SECS_PER_HOUR;
        if (dst_start..dst_end).contains(&utc_secs) {
            2 * SECS_PER_HOUR
        } else {
            SECS_PER_HOUR
        }
    }
}

/// Nombre de jours depuis le 1970-01-01 pour une date du calendrier grégorien
/// proleptique (algorithme `days_from_civil` de Howard Hinnant).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    // L'algorithme fait commencer l'année en mars, pour que le 29 février soit le dernier jour.
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_from_march = (month + 9) % 12;
    let day_of_year = (153 * month_from_march + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Année qui contient le jour donné, compté en jours depuis le 1970-01-01
/// (partie « année » de l'algorithme `civil_from_days` de Howard Hinnant).
fn year_from_days(days: i64) -> i64 {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    // Janvier et février appartiennent à l'année civile suivante.
    let in_next_year = month_from_march >= 10;
    year_of_era + era * 400 + i64::from(in_next_year)
}

/// Jour (en jours depuis le 1970-01-01) du dernier dimanche d'un mois de 31 jours.
fn last_sunday(year: i64, month: i64) -> i64 {
    let last_day = days_from_civil(year, month, 31);
    // Le 1970-01-01 était un jeudi : `(jour + 4) % 7 == 0` correspond donc à un dimanche.
    last_day - (last_day + 4).rem_euclid(7)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Heure de Paris sous forme de triplet, pour garder des assertions courtes.
    fn paris(utc_secs: u64) -> (u8, u8, u8) {
        let t = Time::from_utc(utc_secs, &EuropeParis);
        (t.hours(), t.minutes(), t.seconds())
    }

    #[test]
    fn winter_time_is_utc_plus_1() {
        // 2026-11-17 12:34:56 UTC
        assert_eq!(paris(1_794_918_896), (13, 34, 56));
    }

    #[test]
    fn summer_time_is_utc_plus_2() {
        // 2026-07-14 10:00:00 UTC
        assert_eq!(paris(1_784_023_200), (12, 0, 0));
    }

    #[test]
    fn dst_starts_on_last_sunday_of_march() {
        // 2026-03-29 00:59:59 UTC -> 01:59:59, puis 01:00:00 UTC -> 03:00:00
        assert_eq!(paris(1_774_745_999), (1, 59, 59));
        assert_eq!(paris(1_774_746_000), (3, 0, 0));
    }

    #[test]
    fn dst_ends_on_last_sunday_of_october() {
        // 2026-10-25 00:59:59 UTC -> 02:59:59, puis 01:00:00 UTC -> 02:00:00
        assert_eq!(paris(1_792_889_999), (2, 59, 59));
        assert_eq!(paris(1_792_890_000), (2, 0, 0));
    }

    #[test]
    fn new_year_in_utc_is_already_one_in_paris() {
        // 2026-12-31 23:30:00 UTC -> 2027-01-01 00:30:00
        assert_eq!(paris(1_798_759_800), (0, 30, 0));
    }

    #[test]
    fn new_rejects_out_of_range_fields() {
        assert!(Time::new(23, 59, 59).is_some());
        assert!(Time::new(24, 0, 0).is_none());
        assert!(Time::new(0, 60, 0).is_none());
        assert!(Time::new(0, 0, 60).is_none());
    }

    #[test]
    fn huge_timestamp_does_not_panic() {
        let _ = Time::from_utc(u64::MAX, &EuropeParis);
    }
}
