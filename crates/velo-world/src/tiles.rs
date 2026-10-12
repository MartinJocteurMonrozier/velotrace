//! Tuiles {z}/{x}/{y} téléchargées une fois puis gardées sur disque.

use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Une source de tuiles : nom du dossier de cache et modèle d'URL.
#[derive(Debug, Clone)]
pub struct TileSource {
    pub name: String,
    pub url: String,
}

impl TileSource {
    pub fn new(name: &str, url: &str) -> Self {
        Self {
            name: name.into(),
            url: url.trim().into(),
        }
    }

    pub fn url_for(&self, z: u8, x: u32, y: u32) -> String {
        self.url
            .replace("{z}", &z.to_string())
            .replace("{x}", &x.to_string())
            .replace("{y}", &y.to_string())
    }

    pub fn is_enabled(&self) -> bool {
        !self.url.is_empty()
    }
}

/// Cache disque partagé entre les fils de génération.
pub struct TileCache {
    root: PathBuf,
    agent: Option<ureq::Agent>,
    /// Tuiles absentes du serveur pendant cette session (évite de redemander).
    missing: Mutex<HashMap<String, ()>>,
    /// Une erreur réseau coupe les téléchargements suivants : on travaille
    /// alors avec ce qui est déjà en cache plutôt que d'attendre chaque tuile.
    offline: AtomicBool,
    pub downloaded: AtomicUsize,
    pub failures: AtomicUsize,
}

impl TileCache {
    pub fn new(root: PathBuf, online: bool) -> Self {
        let agent = online.then(|| {
            ureq::AgentBuilder::new()
                .try_proxy_from_env(true)
                .timeout_connect(Duration::from_secs(8))
                .timeout(Duration::from_secs(40))
                .user_agent(concat!("velotrace/", env!("CARGO_PKG_VERSION")))
                .build()
        });
        Self {
            root,
            agent,
            missing: Mutex::new(HashMap::new()),
            offline: AtomicBool::new(false),
            downloaded: AtomicUsize::new(0),
            failures: AtomicUsize::new(0),
        }
    }

    /// Dossier de cache par défaut : ~/.cache/velotrace (Linux), %LOCALAPPDATA%\velotrace (Windows).
    pub fn default_root() -> PathBuf {
        dirs::cache_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("velotrace")
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    fn path_for(&self, src: &TileSource, z: u8, x: u32, y: u32) -> PathBuf {
        self.root
            .join("tuiles")
            .join(&src.name)
            .join(z.to_string())
            .join(x.to_string())
            .join(y.to_string())
    }

    /// Octets de la tuile, depuis le cache ou le réseau ; `None` si elle
    /// n'existe pas ou n'a pas pu être obtenue.
    pub fn get(&self, src: &TileSource, z: u8, x: u32, y: u32) -> Option<Vec<u8>> {
        if !src.is_enabled() {
            return None;
        }
        let path = self.path_for(src, z, x, y);
        if let Ok(bytes) = std::fs::read(&path) {
            return (!bytes.is_empty()).then_some(bytes);
        }
        let url = src.url_for(z, x, y);
        if let Some(local) = url.strip_prefix("file://") {
            return std::fs::read(local).ok();
        }
        let agent = self.agent.as_ref()?;
        if self.offline.load(Ordering::Relaxed) || self.missing.lock().unwrap().contains_key(&url) {
            return None;
        }
        match agent.get(&url).call() {
            Ok(resp) => {
                let mut bytes = Vec::new();
                if resp
                    .into_reader()
                    .take(16 << 20)
                    .read_to_end(&mut bytes)
                    .is_err()
                {
                    self.failures.fetch_add(1, Ordering::Relaxed);
                    return None;
                }
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                // Écriture atomique : un autre fil peut lire la même tuile.
                let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
                if std::fs::write(&tmp, &bytes).is_ok() {
                    let _ = std::fs::rename(&tmp, &path);
                }
                self.downloaded.fetch_add(1, Ordering::Relaxed);
                Some(bytes)
            }
            // Hors couverture : on retient l'absence pour la session.
            Err(ureq::Error::Status(400..=499, _)) => {
                self.missing.lock().unwrap().insert(url, ());
                None
            }
            Err(ureq::Error::Status(..)) => {
                self.failures.fetch_add(1, Ordering::Relaxed);
                None
            }
            Err(ureq::Error::Transport(_)) => {
                self.failures.fetch_add(1, Ordering::Relaxed);
                if self.failures.load(Ordering::Relaxed) >= 3 {
                    self.offline.store(true, Ordering::Relaxed);
                }
                None
            }
        }
    }

    /// Requête POST de formulaire (Overpass) ; renvoie le corps de la réponse.
    pub fn post_form(&self, url: &str, fields: &[(&str, &str)]) -> Result<String, String> {
        let agent = self.agent.as_ref().ok_or("mode hors ligne")?;
        let resp = agent
            .post(url)
            .timeout(Duration::from_secs(200))
            .send_form(fields)
            .map_err(|e| e.to_string())?;
        let mut body = String::new();
        resp.into_reader()
            .take(512 << 20)
            .read_to_string(&mut body)
            .map_err(|e| e.to_string())?;
        Ok(body)
    }

    pub fn is_online(&self) -> bool {
        self.agent.is_some() && !self.offline.load(Ordering::Relaxed)
    }

    /// Télécharge en parallèle ; `progress(fait, total)` est appelé au fil de l'eau.
    pub fn fetch_all(
        &self,
        src: &TileSource,
        tiles: &[(u8, u32, u32)],
        threads: usize,
        progress: &(dyn Fn(usize, usize) + Sync),
    ) -> HashMap<(u32, u32), Vec<u8>> {
        let next = AtomicUsize::new(0);
        let done = AtomicUsize::new(0);
        let out = Mutex::new(HashMap::new());
        std::thread::scope(|s| {
            for _ in 0..threads.max(1) {
                s.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&(z, x, y)) = tiles.get(i) else {
                        break;
                    };
                    if let Some(bytes) = self.get(src, z, x, y) {
                        out.lock().unwrap().insert((x, y), bytes);
                    }
                    progress(done.fetch_add(1, Ordering::Relaxed) + 1, tiles.len());
                });
            }
        });
        out.into_inner().unwrap()
    }
}

/// Raccourci pour partager le cache entre fils.
pub type SharedCache = Arc<TileCache>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_wmts_and_xyz_urls() {
        let s = TileSource::new("t", "https://a/{z}/{x}/{y}.png");
        assert_eq!(s.url_for(13, 4233, 2938), "https://a/13/4233/2938.png");
        let w = TileSource::new("w", "https://w?TILEMATRIX={z}&TILEROW={y}&TILECOL={x}");
        assert_eq!(
            w.url_for(16, 10, 20),
            "https://w?TILEMATRIX=16&TILEROW=20&TILECOL=10"
        );
    }

    #[test]
    fn offline_cache_reads_disk_only() {
        let dir = std::env::temp_dir().join(format!("velotrace-test-{}", std::process::id()));
        let cache = TileCache::new(dir.clone(), false);
        let src = TileSource::new("essai", "https://example.invalid/{z}/{x}/{y}");
        assert!(cache.get(&src, 1, 0, 0).is_none());
        let p = cache.path_for(&src, 1, 0, 0);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(cache.get(&src, 1, 0, 0).as_deref(), Some(&b"abc"[..]));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
