//! Strava pour Godot : connexion au compte (OAuth dans le navigateur) et
//! envoi des sorties enregistrées, chacun dans un fil à part.

use godot::classes::Os;
use godot::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use velo_store::Store;
use velo_strava::{
    authorize_url, default_tokens_path, exchange_code, refresh, upload_fit, wait_upload,
    CodeListener, StravaApp, Tokens,
};

use crate::history::fit_bytes;
use crate::profile;

/// Temps laissé pour autoriser velotrace dans le navigateur.
const AUTHORIZE_TIMEOUT: Duration = Duration::from_secs(300);
/// Temps laissé à Strava pour traiter le fichier envoyé.
const PROCESS_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Default)]
struct State {
    busy: bool,
    message: String,
    progress: f64,
    activity_url: String,
}

#[derive(GodotClass)]
#[class(base = Node)]
pub struct VeloStrava {
    base: Base<Node>,
    state: Arc<Mutex<State>>,
}

#[godot_api]
impl INode for VeloStrava {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            state: Arc::new(Mutex::new(State {
                progress: -1.0,
                ..State::default()
            })),
        }
    }
}

fn app() -> Option<StravaApp> {
    let s = profile::load().strava;
    (!s.client_id.trim().is_empty() && !s.client_secret.trim().is_empty()).then(|| StravaApp {
        client_id: s.client_id.trim().to_string(),
        client_secret: s.client_secret.trim().to_string(),
    })
}

fn tokens() -> Option<Tokens> {
    Tokens::load(default_tokens_path()?).ok().flatten()
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Jeton valide (renouvelé et réenregistré au besoin).
fn fresh_tokens(app: &StravaApp) -> Result<Tokens, String> {
    let mut t = tokens().ok_or("compte Strava non connecté")?;
    if t.needs_refresh(now_unix()) && refresh(app, &mut t).map_err(|e| e.to_string())? {
        if let Some(path) = default_tokens_path() {
            t.save(path).map_err(|e| e.to_string())?;
        }
    }
    Ok(t)
}

impl VeloStrava {
    /// Lance `job` dans un fil, sauf si une opération est déjà en cours.
    fn spawn(
        &mut self,
        start: &str,
        job: impl FnOnce(&Arc<Mutex<State>>) -> Result<String, String> + Send + 'static,
    ) {
        {
            let mut st = self.state.lock().unwrap();
            if st.busy {
                return;
            }
            st.busy = true;
            st.message = start.to_string();
            st.progress = 0.0;
        }
        let state = self.state.clone();
        std::thread::spawn(move || {
            let result = job(&state);
            let mut st = state.lock().unwrap();
            st.busy = false;
            st.progress = -1.0;
            st.message = match result {
                Ok(m) => m,
                Err(e) => format!("Échec : {e}"),
            };
        });
    }
}

fn progress(state: &Arc<Mutex<State>>, p: f64, message: &str) {
    let mut st = state.lock().unwrap();
    st.progress = p;
    st.message = message.to_string();
}

#[godot_api]
impl VeloStrava {
    /// `configured` (identifiants saisis), `connected`, `athlete`, `busy`,
    /// `message`, `progress` (0 à 1, −1 sans opération), `activity_url`.
    #[func]
    fn status(&self) -> VarDictionary {
        let st = self.state.lock().unwrap();
        let t = tokens();
        let mut d = VarDictionary::new();
        d.set("configured", app().is_some());
        d.set("connected", t.is_some());
        d.set(
            "athlete",
            t.as_ref().map_or("", |t| t.athlete_name.as_str()),
        );
        d.set("busy", st.busy);
        d.set("message", st.message.as_str());
        d.set("progress", st.progress);
        d.set("activity_url", st.activity_url.as_str());
        d
    }

    /// Ouvre la page d'autorisation de Strava dans le navigateur, puis attend
    /// le retour sur un port local (résultat dans `status()`).
    #[func]
    fn connect_account(&mut self) {
        let Some(app) = app() else {
            self.state.lock().unwrap().message =
                "Renseigne d'abord le client ID et le secret de ton application API Strava.".into();
            return;
        };
        let listener = match CodeListener::bind() {
            Ok(l) => l,
            Err(e) => {
                self.state.lock().unwrap().message = format!("Échec : {e}");
                return;
            }
        };
        let url = authorize_url(&app, listener.port());
        Os::singleton().shell_open(url.as_str());
        self.spawn("Autorise velotrace dans le navigateur…", move |state| {
            let code = listener
                .wait(AUTHORIZE_TIMEOUT)
                .map_err(|e| e.to_string())?;
            progress(state, 0.5, "Connexion à Strava…");
            let t = exchange_code(&app, &code).map_err(|e| e.to_string())?;
            let path = default_tokens_path().ok_or("dossier de données introuvable")?;
            t.save(path).map_err(|e| e.to_string())?;
            Ok(format!("Connecté à Strava : {}", t.athlete_name))
        });
    }

    #[func]
    fn disconnect_account(&mut self) {
        if let Some(p) = default_tokens_path() {
            let _ = Tokens::forget(p);
        }
        self.state.lock().unwrap().message = "Compte Strava déconnecté.".into();
    }

    /// Envoie une sortie de l'historique (fichier .FIT) sur Strava.
    #[func]
    fn upload_ride(&mut self, ride_id: i64) {
        let Some(app) = app() else {
            self.state.lock().unwrap().message =
                "Strava n'est pas configuré (Paramètres › Strava).".into();
            return;
        };
        self.state.lock().unwrap().activity_url.clear();
        self.spawn("Préparation du fichier…", move |state| {
            let mut store = Store::open_default().map_err(|e| e.to_string())?;
            let (row, bytes) = fit_bytes(&store, ride_id)?;
            let t = fresh_tokens(&app)?;
            progress(state, 0.3, "Envoi sur Strava…");
            let desc = format!(
                "Sortie sur home trainer avec velotrace ({}).",
                row.route_name
            );
            let external = format!("velotrace-{}-{}", row.started_unix, row.id);
            let upload =
                upload_fit(&t, &bytes, &row.name, &desc, &external).map_err(|e| e.to_string())?;
            progress(state, 0.6, "Traitement par Strava…");
            let activity = wait_upload(&t, upload, PROCESS_TIMEOUT).map_err(|e| e.to_string())?;
            store
                .set_strava_activity(ride_id, activity)
                .map_err(|e| e.to_string())?;
            state.lock().unwrap().activity_url =
                format!("https://www.strava.com/activities/{activity}");
            Ok("Sortie envoyée sur Strava.".into())
        });
    }
}
