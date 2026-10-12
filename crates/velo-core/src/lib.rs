//! Cœur de velotrace : tout ce qui ne dépend ni du moteur 3D ni du matériel réel.
//!
//! - [`config`] : profil cycliste, vélo et home trainer (sérialisés en TOML).
//! - [`gearing`] : transmission réelle et changement de vitesse virtuel.
//! - [`physics`] : modèle de forces, intégration de la vitesse, pente effective.
//! - [`route`] : lecture d'un GPX, rééchantillonnage, pentes lissées.
//! - [`ride`] : boucle de simulation d'une sortie.
//! - [`protocol`] : encodage et décodage des trames BLE (FTMS, CSC, HRS).
//! - [`devices`] : interface commune aux appareils réels ou simulés.
//! - [`sim`] : mode test, avec home trainer et accessoires simulés.
//! - [`terrain`] : relief provisoire autour du parcours.

pub mod config;
pub mod devices;
pub mod gearing;
pub mod physics;
pub mod protocol;
pub mod ride;
pub mod route;
pub mod sim;
pub mod terrain;
