//! Décor réel autour d'un parcours : relief (tuiles terrarium), images
//! aériennes (IGN par défaut) et OpenStreetMap (forêts, eau, bâtiments, routes),
//! et la route elle-même d'après OSM (voies, trottoirs, carrefours, ronds-points,
//! murs le long des talus raides), avec arches et barrières de course.
//!
//! Le décor est découpé en deux niveaux :
//! - les « grands carrés » de 2 048 m, peu détaillés, construits au chargement ;
//! - les « carrés proches » de 512 m le long du parcours, détaillés (relief à
//!   4 m, image fine, arbres, bâtiments), générés à la demande autour de la
//!   caméra par des fils de travail et libérés quand on s'éloigne.

pub mod elevation;
pub mod geo;
pub mod mesh;
pub mod osm;
pub mod race;
pub mod raster;
pub mod road;
pub mod roadnet;
pub mod stream;
pub mod tiles;
pub mod world;

pub use stream::Streamer;
pub use world::{ChunkDetail, FarChunk, Quality, World, WorldOptions, FAR_SIZE, NEAR_SIZE};
