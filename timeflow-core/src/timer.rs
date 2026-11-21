//! Déclenchement d'actions périodiques dans une boucle.

use core::time::Duration;

/// Minuterie périodique qui ne bloque pas : on l'interroge à chaque tour de
/// boucle et elle indique si l'échéance est passée.
///
/// Une échéance manquée (boucle ralentie) ne déclenche qu'une seule fois, puis
/// la période repart de l'instant de l'appel : pas de rafale de rattrapage.
/// Avec une période nulle, chaque appel à `is_due` déclenche.
#[derive(Clone, Copy, Debug)]
pub struct Interval {
    period: Duration,
    next: Duration,
}

/// Construction et interrogation de la minuterie.
impl Interval {
    /// Crée une minuterie de période `period`, dont la première échéance
    /// tombe à `start + period`.
    #[must_use]
    pub const fn new(period: Duration, start: Duration) -> Self {
        Self {
            period,
            next: start.saturating_add(period),
        }
    }

    /// Renvoie `true` si l'échéance est passée à l'instant `now` (instant
    /// monotone, par exemple le temps écoulé depuis le démarrage), et prépare
    /// alors l'échéance suivante.
    #[must_use = "ignorer le résultat fait sauter une échéance"]
    pub fn is_due(&mut self, now: Duration) -> bool {
        if now < self.next {
            return false;
        }
        self.next = now.saturating_add(self.period);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    #[test]
    fn fires_once_per_period() {
        let mut timer = Interval::new(secs(10), secs(0));
        assert!(!timer.is_due(secs(9)));
        assert!(timer.is_due(secs(10)));
        assert!(!timer.is_due(secs(15)));
        assert!(timer.is_due(secs(20)));
    }

    #[test]
    fn missed_deadlines_fire_only_once() {
        let mut timer = Interval::new(secs(10), secs(0));
        assert!(timer.is_due(secs(45)));
        assert!(!timer.is_due(secs(46)));
        assert!(timer.is_due(secs(55)));
    }
}
