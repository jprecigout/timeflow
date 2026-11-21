//! Supervision du lien réseau : décide quand (re)lancer une tentative de
//! connexion, indépendamment du pilote Wi-Fi.

use core::time::Duration;

/// État du lien tel que le rapporte le pilote, à un instant donné.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkStatus {
    /// La station n'est pas associée au point d'accès.
    Down,
    /// La station est associée, mais n'a pas encore d'adresse IP (DHCP en
    /// cours).
    Associated,
    /// La station est associée et a une adresse IP : le réseau fonctionne.
    Up,
}

/// Ce que l'appelant doit faire du lien réseau après un appel à
/// [`Reconnector::poll`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkAction {
    /// Rien : le lien est établi, ou une tentative est encore en cours.
    Wait,
    /// Abandonner la tentative en cours et lancer une nouvelle connexion.
    Connect,
}

/// Délais laissés à chaque étape d'une tentative de connexion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkTimeouts {
    /// Temps maximal pour s'associer au point d'accès.
    pub association: Duration,
    /// Temps maximal pour obtenir une adresse IP une fois associé.
    pub address: Duration,
}

/// État interne du [`Reconnector`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinkState {
    /// Le lien est établi.
    Up,
    /// Coupé, et aucune tentative n'a encore été lancée.
    Down,
    /// Tentative lancée à l'instant indiqué, en attente de l'association.
    Associating { since: Duration },
    /// Associé depuis l'instant indiqué, en attente d'une adresse IP.
    AwaitingAddress { since: Duration },
}

/// Machine à états qui relance la connexion quand le lien est coupé.
///
/// Pattern State : la décision dépend du `LinkState` courant, qui évolue à
/// chaque appel de `poll`. Une tentative est abandonnée et relancée si
/// l'association ou l'obtention de l'adresse IP dépasse son délai : une
/// connexion perdue est donc toujours rétablie, sans bloquer l'appelant.
///
/// Les deux étapes ont des délais séparés, car couper une station associée
/// interrompt le DHCP en cours. Avec un signal faible, le DHCP peut prendre
/// plus de dix secondes : un délai unique trop court relancerait la connexion
/// en boucle sans jamais laisser le DHCP aboutir.
#[derive(Debug)]
pub struct Reconnector {
    timeouts: LinkTimeouts,
    state: LinkState,
}

/// Construction et transitions de la machine à états.
impl Reconnector {
    /// Crée un `Reconnector` avec les délais `timeouts`.
    pub const fn new(timeouts: LinkTimeouts) -> Self {
        Self {
            timeouts,
            state: LinkState::Down,
        }
    }

    /// Met à jour l'état d'après l'état actuel du lien et indique quoi faire.
    ///
    /// `now` est un instant monotone (par exemple le temps écoulé depuis le
    /// démarrage) et `status` est l'état du lien rapporté par le pilote.
    pub fn poll(&mut self, now: Duration, status: LinkStatus) -> LinkAction {
        let elapsed = |since: Duration| now.saturating_sub(since);
        match (status, self.state) {
            (LinkStatus::Up, _) => {
                self.state = LinkState::Up;
                LinkAction::Wait
            }
            // Associé : on laisse le DHCP travailler jusqu'à son délai.
            (LinkStatus::Associated, LinkState::AwaitingAddress { since })
                if elapsed(since) < self.timeouts.address =>
            {
                LinkAction::Wait
            }
            (LinkStatus::Associated, LinkState::AwaitingAddress { .. }) => self.restart(now),
            // Vient de s'associer, ou a perdu son adresse IP (bail DHCP
            // expiré) : le délai d'adressage démarre maintenant.
            (LinkStatus::Associated, _) => {
                self.state = LinkState::AwaitingAddress { since: now };
                LinkAction::Wait
            }
            (LinkStatus::Down, LinkState::Associating { since })
                if elapsed(since) < self.timeouts.association =>
            {
                LinkAction::Wait
            }
            (LinkStatus::Down, _) => self.restart(now),
        }
    }

    /// Démarre une nouvelle tentative à l'instant `now`.
    fn restart(&mut self, now: Duration) -> LinkAction {
        self.state = LinkState::Associating { since: now };
        LinkAction::Connect
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use LinkAction::{Connect, Wait};
    use LinkStatus::{Associated, Down, Up};

    const TIMEOUTS: LinkTimeouts = LinkTimeouts {
        association: Duration::from_secs(15),
        address: Duration::from_secs(60),
    };

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    #[test]
    fn connects_at_start_then_waits() {
        let mut r = Reconnector::new(TIMEOUTS);
        assert_eq!(r.poll(secs(0), Down), Connect);
        assert_eq!(r.poll(secs(5), Down), Wait);
        assert_eq!(r.poll(secs(6), Up), Wait);
    }

    #[test]
    fn retries_when_association_times_out() {
        let mut r = Reconnector::new(TIMEOUTS);
        assert_eq!(r.poll(secs(0), Down), Connect);
        assert_eq!(r.poll(secs(14), Down), Wait);
        assert_eq!(r.poll(secs(15), Down), Connect);
        assert_eq!(r.poll(secs(16), Down), Wait);
    }

    #[test]
    fn slow_dhcp_is_not_interrupted() {
        // Cas observé sur la carte : association en 3 s, puis plus de 12 s
        // de DHCP. L'ancien délai unique de 15 s relançait la connexion.
        let mut r = Reconnector::new(TIMEOUTS);
        assert_eq!(r.poll(secs(0), Down), Connect);
        assert_eq!(r.poll(secs(3), Associated), Wait);
        assert_eq!(r.poll(secs(30), Associated), Wait);
        assert_eq!(r.poll(secs(62), Associated), Wait);
        assert_eq!(r.poll(secs(63), Associated), Connect);
    }

    #[test]
    fn address_obtained_within_delay() {
        let mut r = Reconnector::new(TIMEOUTS);
        r.poll(secs(0), Down);
        r.poll(secs(3), Associated);
        assert_eq!(r.poll(secs(20), Up), Wait);
    }

    #[test]
    fn reconnects_when_association_is_lost() {
        let mut r = Reconnector::new(TIMEOUTS);
        r.poll(secs(0), Down);
        r.poll(secs(3), Up);
        assert_eq!(r.poll(secs(3_600), Down), Connect);
    }

    #[test]
    fn reconnects_when_association_drops_during_dhcp() {
        let mut r = Reconnector::new(TIMEOUTS);
        r.poll(secs(0), Down);
        r.poll(secs(3), Associated);
        assert_eq!(r.poll(secs(5), Down), Connect);
    }

    #[test]
    fn already_associated_at_start_waits_for_address() {
        let mut r = Reconnector::new(TIMEOUTS);
        assert_eq!(r.poll(secs(0), Associated), Wait);
        assert_eq!(r.poll(secs(59), Associated), Wait);
        assert_eq!(r.poll(secs(60), Associated), Connect);
    }

    #[test]
    fn address_obtained_directly_from_association() {
        let mut r = Reconnector::new(TIMEOUTS);
        r.poll(secs(0), Down);
        assert_eq!(r.poll(secs(4), Up), Wait);
        assert_eq!(r.poll(secs(100), Up), Wait);
    }

    #[test]
    fn restart_after_dhcp_timeout_waits_for_association() {
        let mut r = Reconnector::new(TIMEOUTS);
        r.poll(secs(0), Down);
        r.poll(secs(3), Associated);
        assert_eq!(r.poll(secs(63), Associated), Connect);
        // La nouvelle tentative a de nouveau 15 s pour s'associer.
        assert_eq!(r.poll(secs(64), Down), Wait);
        assert_eq!(r.poll(secs(77), Down), Wait);
        assert_eq!(r.poll(secs(78), Down), Connect);
    }

    #[test]
    fn lost_address_gives_dhcp_time_to_renew() {
        let mut r = Reconnector::new(TIMEOUTS);
        r.poll(secs(0), Down);
        r.poll(secs(3), Up);
        assert_eq!(r.poll(secs(100), Associated), Wait);
        assert_eq!(r.poll(secs(159), Associated), Wait);
        assert_eq!(r.poll(secs(160), Associated), Connect);
    }
}
