//! Journalisation des pannes répétées, sans inonder les logs.

use core::fmt::Debug;
use log::{info, warn};

/// Retient si une opération exécutée en boucle est actuellement en échec.
///
/// Il journalise un avertissement au premier échec et un message quand
/// l'opération fonctionne de nouveau, mais reste silencieux tant que l'état ne
/// change pas. Une erreur persistante (un fil mal branché, par exemple) ne
/// produit donc pas une ligne de log à chaque tour de boucle.
#[derive(Debug)]
pub struct FailureLatch {
    name: &'static str,
    failing: bool,
}

/// Suivi de l'état d'une opération.
impl FailureLatch {
    /// Crée un verrou pour l'opération nommée `name` (utilisé dans les logs).
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            failing: false,
        }
    }

    /// Enregistre le résultat d'une tentative et renvoie sa valeur en cas de succès.
    pub fn check<T, E: Debug>(&mut self, result: Result<T, E>) -> Option<T> {
        match result {
            Ok(value) => {
                if self.failing {
                    info!("{}: working again", self.name);
                    self.failing = false;
                }
                Some(value)
            }
            Err(e) => {
                if !self.failing {
                    warn!(
                        "{}: failed ({e:?}), further failures are not logged",
                        self.name
                    );
                    self.failing = true;
                }
                None
            }
        }
    }

    /// L'opération est-elle actuellement en échec ?
    pub fn is_failing(&self) -> bool {
        self.failing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_failure_and_recovery() {
        let mut latch = FailureLatch::new("test");
        assert_eq!(latch.check::<_, ()>(Ok(1)), Some(1));
        assert!(!latch.is_failing());
        assert_eq!(latch.check::<i32, _>(Err("boom")), None);
        assert_eq!(latch.check::<i32, _>(Err("boom")), None);
        assert!(latch.is_failing());
        assert_eq!(latch.check::<_, ()>(Ok(2)), Some(2));
        assert!(!latch.is_failing());
    }
}
