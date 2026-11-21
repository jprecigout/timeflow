//! Supervision du lien réseau : décide quand (re)lancer une tentative de
//! connexion, indépendamment du pilote Wi-Fi.

use core::time::Duration;

/// Ce que l'appelant doit faire du lien réseau après un appel à
/// [`Reconnector::poll`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkAction {
    /// Rien : le lien est établi, ou une tentative est encore en cours.
    Wait,
    /// Abandonner la tentative en cours et lancer une nouvelle connexion.
    Connect,
}

/// État du lien vu par le [`Reconnector`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinkState {
    /// Le lien est établi.
    Up,
    /// Coupé, et aucune tentative n'a encore été lancée.
    Down,
    /// Coupé, avec une tentative lancée à l'instant indiqué.
    Connecting { since: Duration },
}

/// Machine à états qui relance la connexion quand le lien est coupé.
///
/// Pattern State : la décision dépend du `LinkState` courant, qui évolue à
/// chaque appel de `poll`. Une tentative qui n'établit pas le lien en moins de
/// `timeout` est abandonnée et une nouvelle est lancée : une connexion perdue
/// est donc toujours rétablie, sans bloquer l'appelant.
#[derive(Debug)]
pub struct Reconnector {
    timeout: Duration,
    state: LinkState,
}

/// Construction et transitions de la machine à états.
impl Reconnector {
    /// Crée un `Reconnector` qui laisse `timeout` à chaque tentative pour réussir.
    pub const fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            state: LinkState::Down,
        }
    }

    /// Met à jour l'état d'après l'état actuel du lien et indique quoi faire.
    ///
    /// `now` est un instant monotone (par exemple le temps écoulé depuis le
    /// démarrage) et `is_up` indique si le lien fonctionne en ce moment.
    pub fn poll(&mut self, now: Duration, is_up: bool) -> LinkAction {
        if is_up {
            self.state = LinkState::Up;
            return LinkAction::Wait;
        }
        match self.state {
            LinkState::Connecting { since } if now.saturating_sub(since) < self.timeout => {
                LinkAction::Wait
            }
            LinkState::Up | LinkState::Down | LinkState::Connecting { .. } => {
                self.state = LinkState::Connecting { since: now };
                LinkAction::Connect
            }
        }
    }

    /// Le lien était-il établi lors du dernier appel de `poll` ?
    pub fn is_up(&self) -> bool {
        self.state == LinkState::Up
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TIMEOUT: Duration = Duration::from_secs(15);

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    #[test]
    fn connects_at_start_then_waits() {
        let mut r = Reconnector::new(TIMEOUT);
        assert_eq!(r.poll(secs(0), false), LinkAction::Connect);
        assert_eq!(r.poll(secs(5), false), LinkAction::Wait);
        assert_eq!(r.poll(secs(6), true), LinkAction::Wait);
        assert!(r.is_up());
    }

    #[test]
    fn retries_after_timeout() {
        let mut r = Reconnector::new(TIMEOUT);
        assert_eq!(r.poll(secs(0), false), LinkAction::Connect);
        assert_eq!(r.poll(secs(14), false), LinkAction::Wait);
        assert_eq!(r.poll(secs(15), false), LinkAction::Connect);
        assert_eq!(r.poll(secs(16), false), LinkAction::Wait);
    }

    #[test]
    fn reconnects_when_link_is_lost() {
        let mut r = Reconnector::new(TIMEOUT);
        r.poll(secs(0), false);
        r.poll(secs(3), true);
        assert_eq!(r.poll(secs(3_600), false), LinkAction::Connect);
        assert!(!r.is_up());
    }
}
