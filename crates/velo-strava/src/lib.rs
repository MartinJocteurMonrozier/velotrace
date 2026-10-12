//! Envoi des sorties de velotrace vers Strava.
//!
//! Tout est bloquant : l'appelant (le pont Godot) lance ces fonctions dans un
//! fil à part.
//!
//! Déroulé de la connexion :
//! 1. [`CodeListener::bind`] ouvre un petit serveur HTTP local sur un port libre ;
//! 2. on ouvre dans le navigateur [`authorize_url`] avec ce port ;
//! 3. [`CodeListener::wait`] reçoit le retour de Strava et en tire le code ;
//! 4. [`exchange_code`] échange ce code contre des [`Tokens`], que l'appelant
//!    garde avec [`Tokens::save`].
//!
//! Envoi d'une sortie : [`refresh`] si besoin, [`upload_fit`], puis
//! [`wait_upload`] jusqu'à obtenir l'identifiant de l'activité.

mod listener;
mod tokens;
mod upload;

pub use listener::CodeListener;
pub use tokens::{default_tokens_path, Tokens};
pub use upload::{
    multipart_body, parse_upload_response, upload_fit, upload_status, wait_upload, UploadStatus,
};

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

const AUTHORIZE_URL: &str = "https://www.strava.com/oauth/authorize";
const TOKEN_URL: &str = "https://www.strava.com/oauth/token";
const API_URL: &str = "https://www.strava.com/api/v3";
/// Droits demandés : lire le profil et créer des activités.
pub const SCOPE: &str = "activity:write,read";
/// Un jeton qui expire dans moins de 60 s est renouvelé avant usage.
const REFRESH_MARGIN_S: i64 = 60;

/// Identifiants de l'application déclarée sur strava.com/settings/api.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StravaApp {
    pub client_id: String,
    pub client_secret: String,
}

/// Erreurs de l'échange avec Strava.
#[derive(Debug, thiserror::Error)]
pub enum StravaError {
    #[error("impossible de joindre Strava : {0}")]
    Network(String),
    #[error("connexion à Strava refusée")]
    AuthorizationDenied,
    #[error("Strava n'a pas accordé le droit d'envoyer des activités : coche la case lors de la connexion")]
    MissingScope,
    #[error("Strava a répondu {status} : {message}")]
    Http { status: u16, message: String },
    #[error("Strava a refusé l'activité : {0}")]
    UploadRejected(String),
    #[error("Strava ne répond pas : délai dépassé")]
    Timeout,
    #[error("réponse inattendue de Strava : {0}")]
    InvalidResponse(String),
    #[error("erreur de fichier ou de connexion locale : {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, StravaError>;

/// Adresse de la page d'autorisation Strava, qui renverra le navigateur vers
/// `http://localhost:{redirect_port}/`.
pub fn authorize_url(app: &StravaApp, redirect_port: u16) -> String {
    let redirect = format!("http://localhost:{redirect_port}/");
    format!(
        "{AUTHORIZE_URL}?client_id={}&response_type=code&redirect_uri={}&approval_prompt=auto&scope={}",
        percent_encode(&app.client_id),
        percent_encode(&redirect),
        percent_encode(SCOPE),
    )
}

/// Échange le code reçu par [`CodeListener::wait`] contre des jetons.
pub fn exchange_code(app: &StravaApp, code: &str) -> Result<Tokens> {
    let body = post_form(&[
        ("client_id", &app.client_id),
        ("client_secret", &app.client_secret),
        ("code", code),
        ("grant_type", "authorization_code"),
    ])?;
    parse_token_response(&body, None)
}

/// Renouvelle les jetons s'ils expirent dans moins d'une minute ; renvoie
/// vrai s'ils ont changé (l'appelant doit alors les réenregistrer).
pub fn refresh(app: &StravaApp, tokens: &mut Tokens) -> Result<bool> {
    if !tokens.needs_refresh(now_unix()) {
        return Ok(false);
    }
    let body = post_form(&[
        ("client_id", &app.client_id),
        ("client_secret", &app.client_secret),
        ("refresh_token", &tokens.refresh_token),
        ("grant_type", "refresh_token"),
    ])?;
    *tokens = parse_token_response(&body, Some(tokens))?;
    Ok(true)
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
    athlete: Option<Athlete>,
}

#[derive(Deserialize)]
struct Athlete {
    firstname: Option<String>,
    lastname: Option<String>,
}

/// Lit la réponse JSON de `/oauth/token`. Le renouvellement ne renvoie pas
/// l'athlète : on garde alors le nom des jetons précédents.
pub fn parse_token_response(json: &str, previous: Option<&Tokens>) -> Result<Tokens> {
    let r: TokenResponse =
        serde_json::from_str(json).map_err(|e| StravaError::InvalidResponse(e.to_string()))?;
    let athlete_name = match r.athlete {
        Some(a) => [a.firstname, a.lastname]
            .into_iter()
            .flatten()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
        None => previous.map(|t| t.athlete_name.clone()).unwrap_or_default(),
    };
    Ok(Tokens {
        access_token: r.access_token,
        refresh_token: r.refresh_token,
        expires_at: r.expires_at,
        athlete_name,
    })
}

pub(crate) fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

pub(crate) fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .try_proxy_from_env(true)
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("velotrace/", env!("CARGO_PKG_VERSION")))
        .build()
}

fn post_form(form: &[(&str, &str)]) -> Result<String> {
    let resp = agent().post(TOKEN_URL).send_form(form).map_err(map_error)?;
    resp.into_string()
        .map_err(|e| StravaError::Network(e.to_string()))
}

/// Traduit une erreur ureq, en reprenant le message que Strava renvoie
/// (`{"message": "...", "errors": [...]}`).
pub(crate) fn map_error(e: ureq::Error) -> StravaError {
    match e {
        ureq::Error::Status(status, resp) => {
            let body = resp.into_string().unwrap_or_default();
            StravaError::Http {
                status,
                message: strava_message(&body),
            }
        }
        ureq::Error::Transport(t) => StravaError::Network(t.to_string()),
    }
}

/// Message lisible d'une réponse d'erreur Strava.
pub fn strava_message(body: &str) -> String {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
        return body.trim().chars().take(200).collect();
    };
    let mut msg = v["message"].as_str().unwrap_or_default().to_string();
    if let Some(errors) = v["errors"].as_array() {
        let details: Vec<String> = errors
            .iter()
            .filter_map(|e| {
                let field = e["field"].as_str().unwrap_or_default();
                let code = e["code"].as_str().unwrap_or_default();
                (!field.is_empty() || !code.is_empty()).then(|| format!("{field} {code}"))
            })
            .map(|s| s.trim().to_string())
            .collect();
        if !details.is_empty() {
            msg = format!("{msg} ({})", details.join(", "));
        }
    }
    msg
}

/// Encodage « pourcent » des composants d'URL (RFC 3986).
pub(crate) fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Décodage d'un composant de chaîne de requête (`+` vaut une espace).
pub(crate) fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 1 < bytes.len() => {
                match (hex(bytes[i + 1]), bytes.get(i + 2).and_then(|c| hex(*c))) {
                    (Some(h), Some(l)) => {
                        out.push((h * 16 + l) as u8);
                        i += 2;
                    }
                    _ => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests;
