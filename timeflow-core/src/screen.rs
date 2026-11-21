//! Abstraction de l'afficheur, pour que la logique de l'horloge ne dépende pas
//! du pilote MAX7219.

use crate::frame::Frame;

/// Tout ce qui sait afficher une image complète.
///
/// Le firmware l'implémente avec un Adapter autour du pilote MAX7219 ; les tests
/// peuvent l'implémenter avec un simple enregistreur.
pub trait Screen {
    /// Erreur renvoyée quand l'image n'a pas pu être affichée.
    type Error;

    /// Affiche `frame`.
    ///
    /// # Errors
    ///
    /// Renvoie `Self::Error` si l'image n'a pas pu être envoyée à l'afficheur.
    fn draw(&mut self, frame: &Frame) -> Result<(), Self::Error>;
}

/// Écran qui ne transmet une image que si elle diffère de la dernière affichée.
///
/// Pattern Decorator : il enveloppe un autre `Screen` et lui ajoute un cache
/// sans changer son interface. Le cache n'est mis à jour qu'en cas de succès :
/// une image dont l'écriture a échoué est renvoyée à l'appel suivant.
#[derive(Debug)]
pub struct CachedScreen<S> {
    inner: S,
    last: Option<Frame>,
}

/// Construction du cache.
impl<S: Screen> CachedScreen<S> {
    /// Enveloppe `inner` ; la première image est toujours transmise.
    pub fn new(inner: S) -> Self {
        Self { inner, last: None }
    }
}

/// Affichage avec cache : l'image n'est transmise que si elle a changé.
impl<S: Screen> Screen for CachedScreen<S> {
    type Error = S::Error;

    fn draw(&mut self, frame: &Frame) -> Result<(), Self::Error> {
        if self.last.as_ref() == Some(frame) {
            return Ok(());
        }
        self.inner.draw(frame)?;
        self.last = Some(*frame);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Doublure de test : enregistre les images reçues et peut être mise en échec.
    #[derive(Default)]
    struct Recorder {
        frames: Vec<Frame>,
        fail: bool,
    }

    impl Screen for &mut Recorder {
        type Error = ();

        fn draw(&mut self, frame: &Frame) -> Result<(), ()> {
            if self.fail {
                return Err(());
            }
            self.frames.push(*frame);
            Ok(())
        }
    }

    #[test]
    fn identical_frames_are_written_once() {
        let mut recorder = Recorder::default();
        let mut screen = CachedScreen::new(&mut recorder);
        let a = [1; 8];
        let b = [2; 8];
        for frame in [a, a, b, b, a] {
            screen.draw(&frame).unwrap();
        }
        assert_eq!(recorder.frames, vec![a, b, a]);
    }

    #[test]
    fn failed_frame_is_retried() {
        let mut recorder = Recorder {
            fail: true,
            ..Recorder::default()
        };
        let frame = [1; 8];
        {
            let mut screen = CachedScreen::new(&mut recorder);
            assert!(screen.draw(&frame).is_err());
            screen.inner.fail = false;
            screen.draw(&frame).unwrap();
        }
        assert_eq!(recorder.frames, vec![frame]);
    }
}
