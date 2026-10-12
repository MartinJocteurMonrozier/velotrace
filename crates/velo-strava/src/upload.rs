//! Téléversement d'un fichier FIT sur Strava et suivi de son traitement.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use crate::{agent, map_error, Result, StravaError, Tokens, API_URL};

/// Intervalle entre deux demandes d'état du téléversement.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// État du traitement d'un téléversement par Strava.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadStatus {
    /// Strava traite encore le fichier.
    Processing,
    /// Activité créée (ou déjà présente : doublon), avec son identifiant.
    Ready(u64),
    /// Fichier refusé, avec le message de Strava.
    Error(String),
}

/// Envoie un fichier FIT ; renvoie l'identifiant du téléversement, à suivre
/// avec [`upload_status`] ou [`wait_upload`].
pub fn upload_fit(
    tokens: &Tokens,
    fit_bytes: &[u8],
    name: &str,
    description: &str,
    external_id: &str,
) -> Result<u64> {
    let boundary = new_boundary();
    let body = multipart_body(
        &boundary,
        &[
            ("data_type", "fit"),
            ("name", name),
            ("description", description),
            ("trainer", "1"),
            ("sport_type", "VirtualRide"),
            ("external_id", external_id),
        ],
        ("file", "activity.fit", fit_bytes),
    );
    let resp = agent()
        .post(&format!("{API_URL}/uploads"))
        .set("Authorization", &format!("Bearer {}", tokens.access_token))
        .set(
            "Content-Type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .send_bytes(&body)
        .map_err(map_error)?;
    let text = resp
        .into_string()
        .map_err(|e| StravaError::Network(e.to_string()))?;
    Ok(parse_upload_response(&text)?.0)
}

/// État courant d'un téléversement.
pub fn upload_status(tokens: &Tokens, upload_id: u64) -> Result<UploadStatus> {
    let resp = agent()
        .get(&format!("{API_URL}/uploads/{upload_id}"))
        .set("Authorization", &format!("Bearer {}", tokens.access_token))
        .call()
        .map_err(map_error)?;
    let text = resp
        .into_string()
        .map_err(|e| StravaError::Network(e.to_string()))?;
    Ok(parse_upload_response(&text)?.1)
}

/// Interroge Strava toutes les 2 s jusqu'à ce que l'activité soit créée ;
/// renvoie son identifiant.
pub fn wait_upload(tokens: &Tokens, upload_id: u64, timeout: Duration) -> Result<u64> {
    let deadline = Instant::now() + timeout;
    loop {
        match upload_status(tokens, upload_id)? {
            UploadStatus::Ready(id) => return Ok(id),
            UploadStatus::Error(msg) => return Err(StravaError::UploadRejected(msg)),
            UploadStatus::Processing => {}
        }
        let now = Instant::now();
        if now >= deadline {
            return Err(StravaError::Timeout);
        }
        std::thread::sleep(POLL_INTERVAL.min(deadline - now));
    }
}

#[derive(Deserialize)]
struct UploadResponse {
    id: u64,
    error: Option<String>,
    activity_id: Option<u64>,
}

/// Lit la réponse JSON de `/uploads` : `(identifiant du téléversement, état)`.
/// Un doublon (« duplicate of activity 123 ») compte comme prêt.
pub fn parse_upload_response(json: &str) -> Result<(u64, UploadStatus)> {
    let r: UploadResponse =
        serde_json::from_str(json).map_err(|e| StravaError::InvalidResponse(e.to_string()))?;
    let status = match (r.error.filter(|e| !e.trim().is_empty()), r.activity_id) {
        (Some(err), _) => match duplicate_of(&err) {
            Some(id) => UploadStatus::Ready(id),
            None => UploadStatus::Error(err),
        },
        (None, Some(id)) => UploadStatus::Ready(id),
        (None, None) => UploadStatus::Processing,
    };
    Ok((r.id, status))
}

/// Identifiant de l'activité existante dans un message de doublon Strava,
/// par exemple « activity.fit duplicate of activity 123 » ou
/// « duplicate of <a href='/activities/123'>123</a> ».
pub(crate) fn duplicate_of(error: &str) -> Option<u64> {
    let lower = error.to_ascii_lowercase();
    let rest = &error[lower.find("duplicate of")? + "duplicate of".len()..];
    let start = rest.find(|c: char| c.is_ascii_digit())?;
    let digits: String = rest[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// Corps `multipart/form-data` : les champs texte puis le fichier
/// `(nom du champ, nom du fichier, octets)`.
pub fn multipart_body(
    boundary: &str,
    fields: &[(&str, &str)],
    file: (&str, &str, &[u8]),
) -> Vec<u8> {
    let mut body = Vec::with_capacity(file.2.len() + 256 * (fields.len() + 1));
    for (name, value) in fields {
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{}\"\r\n\r\n{value}\r\n",
                escape_quotes(name)
            )
            .as_bytes(),
        );
    }
    let (name, filename, bytes) = file;
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n\
             Content-Type: application/octet-stream\r\n\r\n",
            escape_quotes(name),
            escape_quotes(filename)
        )
        .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

fn escape_quotes(s: &str) -> String {
    s.replace('"', "%22").replace(['\r', '\n'], " ")
}

/// Frontière assez aléatoire pour ne pas apparaître dans le fichier.
fn new_boundary() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("----velotrace{:x}{:x}", nanos, std::process::id())
}
