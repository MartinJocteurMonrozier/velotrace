//! Petit serveur HTTP local qui reçoit le retour de la page d'autorisation
//! Strava (`GET /?code=...&scope=...` ou `GET /?error=access_denied`).

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use crate::{percent_decode, Result, StravaError};

const PAGE_OK: &str =
    "Connexion à Strava réussie, tu peux fermer cet onglet et revenir à velotrace.";
const PAGE_DENIED: &str =
    "Connexion à Strava refusée. Tu peux fermer cet onglet et réessayer depuis velotrace.";
const PAGE_SCOPE: &str = "velotrace n'a pas reçu le droit d'envoyer tes activités : réessaie en laissant cette case cochée.";

/// Écoute sur 127.0.0.1, sur un port choisi par le système.
pub struct CodeListener {
    listener: TcpListener,
    port: u16,
}

/// Ce que contient une requête reçue par le serveur local.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Callback {
    Code {
        code: String,
        scope: Option<String>,
    },
    Error(String),
    /// Autre requête (icône du site…), ignorée.
    Other,
}

impl CodeListener {
    /// Ouvre le serveur local ; [`CodeListener::port`] donne le port à passer
    /// à [`crate::authorize_url`].
    pub fn bind() -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;
        Ok(Self { listener, port })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Attend le retour du navigateur et renvoie le code d'autorisation.
    /// Le navigateur reçoit une petite page en français.
    pub fn wait(&self, timeout: Duration) -> Result<String> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if let Some(result) = handle(stream) {
                        return result;
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(StravaError::Timeout);
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
}

/// Traite une connexion ; `None` si ce n'était pas le retour attendu.
fn handle(mut stream: TcpStream) -> Option<Result<String>> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let request = read_head(&mut stream);
    let (status, page, result) = match parse_request(&request) {
        Callback::Code { scope, .. }
            if scope
                .as_deref()
                .is_some_and(|s| !s.split(',').any(|x| x.trim() == "activity:write")) =>
        {
            ("200 OK", PAGE_SCOPE, Some(Err(StravaError::MissingScope)))
        }
        Callback::Code { code, .. } => ("200 OK", PAGE_OK, Some(Ok(code))),
        Callback::Error(_) => (
            "200 OK",
            PAGE_DENIED,
            Some(Err(StravaError::AuthorizationDenied)),
        ),
        Callback::Other => ("404 Not Found", "", None),
    };
    let body = if page.is_empty() {
        String::new()
    } else {
        html_page(page)
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
    result
}

/// Lit la requête jusqu'à la ligne vide (8 Ko au plus).
fn read_head(stream: &mut TcpStream) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    while buf.len() < 8192 {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// Analyse la ligne de requête `GET /?code=...&scope=... HTTP/1.1`.
pub(crate) fn parse_request(request: &str) -> Callback {
    let line = request.lines().next().unwrap_or_default();
    let mut parts = line.split_whitespace();
    if parts.next() != Some("GET") {
        return Callback::Other;
    }
    let target = parts.next().unwrap_or_default();
    let Some((path, query)) = target.split_once('?') else {
        return Callback::Other;
    };
    if path != "/" {
        return Callback::Other;
    }
    let (mut code, mut scope, mut error) = (None, None, None);
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let v = percent_decode(v);
        match k {
            "code" => code = Some(v),
            "scope" => scope = Some(v),
            "error" => error = Some(v),
            _ => {}
        }
    }
    match (error, code) {
        (Some(e), _) => Callback::Error(e),
        (None, Some(c)) if !c.is_empty() => Callback::Code { code: c, scope },
        _ => Callback::Other,
    }
}

fn html_page(message: &str) -> String {
    format!(
        "<!doctype html><html lang=\"fr\"><head><meta charset=\"utf-8\"><title>velotrace</title>\
         <style>body{{font-family:sans-serif;background:#1b1d22;color:#eee;display:flex;\
         align-items:center;justify-content:center;height:100vh;margin:0}}\
         p{{font-size:1.3em;max-width:32em;text-align:center}}</style></head>\
         <body><p>{message}</p></body></html>"
    )
}
