//! Jetons d'accès Strava, gardés en JSON dans le dossier de données.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Result, StravaError, REFRESH_MARGIN_S};

/// Jetons OAuth d'un athlète connecté.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    /// Expiration du jeton d'accès, en secondes depuis le 1er janvier 1970 (UTC).
    pub expires_at: i64,
    /// Prénom et nom de l'athlète, pour l'affichage.
    #[serde(default)]
    pub athlete_name: String,
}

/// Fichier des jetons par défaut : ~/.local/share/velotrace/strava.json
/// (Linux), %APPDATA%\velotrace\strava.json (Windows).
pub fn default_tokens_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("velotrace").join("strava.json"))
}

impl Tokens {
    /// Vrai si le jeton d'accès expire dans moins d'une minute.
    pub fn needs_refresh(&self, now_unix: i64) -> bool {
        self.expires_at < now_unix + REFRESH_MARGIN_S
    }

    /// Lit les jetons ; `None` si le fichier n'existe pas (pas encore connecté).
    pub fn load(path: impl AsRef<Path>) -> Result<Option<Tokens>> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| StravaError::InvalidResponse(format!("fichier des jetons : {e}")))
    }

    /// Enregistre les jetons, lisibles par le seul utilisateur (0600 sous
    /// Unix). L'écriture passe par un fichier temporaire renommé.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| StravaError::InvalidResponse(e.to_string()))?;
        let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        {
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            let mut f = opts.open(&tmp)?;
            f.write_all(json.as_bytes())?;
            f.sync_all()?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Supprime le fichier des jetons (déconnexion).
    pub fn forget(path: impl AsRef<Path>) -> Result<()> {
        match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }
}
