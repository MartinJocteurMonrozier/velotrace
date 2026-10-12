use crate::devices::RememberedDevice;
use serde::{Deserialize, Serialize};

/// Profil complet, édité depuis l'interface et sauvegardé en TOML.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    pub rider: Rider,
    pub bike: Bike,
    pub trainer: Trainer,
    #[serde(default)]
    pub app: AppSettings,
    #[serde(default)]
    pub scenery: Scenery,
    /// Appareils appairés, un par rôle au plus.
    #[serde(default)]
    pub devices: Vec<RememberedDevice>,
    #[serde(default)]
    pub strava: StravaSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppSettings {
    /// Mode test : home trainer, cadence, FC et Click simulés, sans Bluetooth.
    pub test_mode: bool,
    /// Caméra au départ : 0 proche, 1 arrière, 2 de côté, 3 haute.
    pub camera: u8,
    /// Décor : « reel », « cache » ou « simple ».
    pub decor: String,
    /// Click automatiques au départ (changement de vitesse virtuel).
    pub auto_shift: bool,
    /// Cycliste virtuel du mode test : « cadence » ou « puissance ».
    pub test_rider: String,
    /// Cadence (rpm) ou puissance (W) du cycliste virtuel.
    pub test_value: f64,
    /// Dernier parcours ouvert.
    pub last_gpx: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            test_mode: false,
            camera: 0,
            decor: "reel".into(),
            auto_shift: false,
            test_rider: "cadence".into(),
            test_value: 88.0,
            last_gpx: String::new(),
        }
    }
}

/// Envoi des sorties sur Strava. Il faut créer une « application API » sur
/// https://www.strava.com/settings/api (domaine de rappel : localhost).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct StravaSettings {
    pub client_id: String,
    pub client_secret: String,
    /// Envoie chaque sortie enregistrée sans le demander.
    pub auto_upload: bool,
}

/// Décor : relief, images aériennes et données OpenStreetMap.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Scenery {
    /// 0 = basse, 1 = moyenne, 2 = haute.
    pub quality: u8,
    /// Télécharge ce qui manque ; sinon on se contente du cache.
    pub online: bool,
    /// Remplace l'altitude du GPX par celle du relief (plus régulière).
    pub use_dem_elevation: bool,
    /// Tuiles d'altitude « terrarium » : {z}, {x}, {y}.
    pub elevation_url: String,
    /// Tuiles d'images aériennes (WMTS ou XYZ) : {z}, {x}, {y}. Vide = pas d'images.
    pub imagery_url: String,
    /// Serveur Overpass pour les forêts, bâtiments, eau et routes.
    pub overpass_url: String,
    /// Dossier du cache ; vide = dossier de cache de l'utilisateur.
    pub cache_dir: String,
}

impl Default for Scenery {
    fn default() -> Self {
        Self {
            quality: 1,
            online: true,
            use_dem_elevation: true,
            elevation_url:
                "https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png".into(),
            imagery_url: "https://data.geopf.fr/wmts?SERVICE=WMTS&REQUEST=GetTile&VERSION=1.0.0\
                          &LAYER=ORTHOIMAGERY.ORTHOPHOTOS&STYLE=normal&TILEMATRIXSET=PM\
                          &TILEMATRIX={z}&TILEROW={y}&TILECOL={x}&FORMAT=image/jpeg"
                .into(),
            overpass_url: "https://overpass-api.de/api/interpreter".into(),
            cache_dir: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Rider {
    pub name: String,
    pub weight_kg: f64,
    pub ftp_w: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bike {
    pub name: String,
    pub weight_kg: f64,
    /// Plateaux, du plus petit au plus grand.
    pub chainrings: Vec<u32>,
    /// Pignons, du plus petit au plus grand.
    pub cassette: Vec<u32>,
    /// Circonférence de la roue en mètres (700×28c ≈ 2,136 m, 700×40c ≈ 2,200 m).
    pub wheel_circumference_m: f64,
    pub cda_m2: f64,
    pub crr: f64,
    /// Rendement de la transmission (0,975 = 2,5 % de pertes).
    pub drivetrain_efficiency: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Trainer {
    /// Rapport physique sur lequel reste la chaîne en changement virtuel.
    pub physical_chainring: u32,
    pub physical_cog: u32,
    /// Difficulté des pentes, 1,0 = réalité.
    pub grade_difficulty: f64,
    /// Plage de pente que l'on s'autorise à envoyer au trainer.
    pub min_grade: f64,
    pub max_grade: f64,
}

impl Profile {
    /// Profil de Martin : 52/36, 11-34 11v, pneus 700×40c, 59 kg + 11 kg, Click V2.
    pub fn martin() -> Self {
        Self {
            rider: Rider {
                name: "Martin".into(),
                weight_kg: 59.0,
                ftp_w: None,
            },
            bike: Bike {
                name: "Route".into(),
                weight_kg: 11.0,
                chainrings: vec![36, 52],
                cassette: vec![11, 13, 15, 17, 19, 21, 23, 25, 27, 30, 34],
                wheel_circumference_m: 2.200,
                cda_m2: 0.32,
                crr: 0.004,
                drivetrain_efficiency: 0.975,
            },
            trainer: Trainer {
                physical_chainring: 52,
                physical_cog: 19,
                grade_difficulty: 1.0,
                min_grade: -0.10,
                max_grade: 0.20,
            },
            app: AppSettings::default(),
            scenery: Scenery::default(),
            devices: Vec::new(),
            strava: StravaSettings::default(),
        }
    }

    /// FTP utilisée pour les séances et la charge d'entraînement (200 W par défaut).
    pub fn ftp(&self) -> f64 {
        self.rider.ftp_w.unwrap_or(200) as f64
    }

    pub fn total_mass_kg(&self) -> f64 {
        self.rider.weight_kg + self.bike.weight_kg
    }

    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    pub fn from_toml(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_round_trip() {
        let p = Profile::martin();
        let back = Profile::from_toml(&p.to_toml().unwrap()).unwrap();
        assert_eq!(p, back);
        assert_eq!(p.total_mass_kg(), 70.0);
    }

    #[test]
    fn old_profiles_get_default_scenery() {
        let mut p = Profile::martin();
        let text = p.to_toml().unwrap();
        let cut = text.find("[scenery]").unwrap();
        let back = Profile::from_toml(&text[..cut]).unwrap();
        p.scenery = Scenery::default();
        assert_eq!(back, p);
        assert!(p
            .scenery
            .imagery_url
            .contains("TILEMATRIX={z}&TILEROW={y}&TILECOL={x}"));
    }
}
