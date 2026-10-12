use serde::{Deserialize, Serialize};

/// Profil complet, édité depuis l'interface et sauvegardé en TOML.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    pub rider: Rider,
    pub bike: Bike,
    pub trainer: Trainer,
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
    /// Circonférence de la roue en mètres (700×28c ≈ 2,136 m).
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
    /// Profil de Martin : 52/36, 11-34 11v, 59 kg + 11 kg, Click V2.
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
                wheel_circumference_m: 2.136,
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
        }
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
}
