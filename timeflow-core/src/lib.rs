//! Logique de l'horloge TimeFlow, indépendante du matériel.
//!
//! Ce crate ne dépend pas de l'ESP32 : il compile et se teste sur PC
//! (`cargo test-core`). Le firmware se contente de brancher le matériel sur les
//! abstractions définies ici.
//!
//! Le crate est `no_std` hors tests : il n'alloue rien et n'utilise que `core`
//! et `log`. Les tests, eux, ont accès à `std`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod frame;
pub mod health;
pub mod link;
pub mod screen;
pub mod time;
pub mod timer;
