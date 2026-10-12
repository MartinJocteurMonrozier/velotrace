use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use super::*;
use crate::listener::{parse_request, Callback};
use crate::upload::duplicate_of;

fn app() -> StravaApp {
    StravaApp {
        client_id: "12345".into(),
        client_secret: "secret".into(),
    }
}

#[test]
fn authorize_url_is_encoded() {
    let url = authorize_url(&app(), 43123);
    assert_eq!(
        url,
        "https://www.strava.com/oauth/authorize?client_id=12345&response_type=code\
         &redirect_uri=http%3A%2F%2Flocalhost%3A43123%2F&approval_prompt=auto\
         &scope=activity%3Awrite%2Cread"
    );
}

#[test]
fn percent_coding_round_trips() {
    let s = "Été à l'Alpe d'Huez : 21 lacets & 1100 m+";
    assert_eq!(percent_decode(&percent_encode(s)), s);
    assert_eq!(percent_decode("a+b%2Cc%"), "a b,c%");
    assert_eq!(percent_decode("%zz"), "%zz");
}

#[test]
fn token_response_is_parsed() {
    let json = r#"{
        "token_type": "Bearer",
        "expires_at": 1730000000,
        "expires_in": 21600,
        "refresh_token": "r-123",
        "access_token": "a-456",
        "athlete": {"id": 42, "username": "mm", "firstname": "Martin", "lastname": "M."}
    }"#;
    let t = parse_token_response(json, None).unwrap();
    assert_eq!(
        t,
        Tokens {
            access_token: "a-456".into(),
            refresh_token: "r-123".into(),
            expires_at: 1_730_000_000,
            athlete_name: "Martin M.".into(),
        }
    );
    assert!(t.needs_refresh(1_730_000_000 - 30));
    assert!(!t.needs_refresh(1_730_000_000 - 3600));

    // Le renouvellement ne renvoie pas l'athlète : le nom est conservé.
    let refreshed = r#"{"token_type":"Bearer","access_token":"a-789","expires_at":1730021600,
        "expires_in":21600,"refresh_token":"r-124"}"#;
    let t2 = parse_token_response(refreshed, Some(&t)).unwrap();
    assert_eq!(t2.access_token, "a-789");
    assert_eq!(t2.refresh_token, "r-124");
    assert_eq!(t2.athlete_name, "Martin M.");

    assert!(matches!(
        parse_token_response(r#"{"message":"Bad Request"}"#, None),
        Err(StravaError::InvalidResponse(_))
    ));
}

#[test]
fn strava_error_messages() {
    let body = r#"{"message":"Bad Request","errors":[{"resource":"AuthorizationCode","field":"code","code":"invalid"}]}"#;
    assert_eq!(strava_message(body), "Bad Request (code invalid)");
    assert_eq!(strava_message("<html>oups</html>"), "<html>oups</html>");
    let e = StravaError::Http {
        status: 401,
        message: "Authorization Error".into(),
    };
    assert_eq!(e.to_string(), "Strava a répondu 401 : Authorization Error");
}

#[test]
fn tokens_file_round_trip() {
    let dir = std::env::temp_dir().join(format!("velo-strava-test-{}", std::process::id()));
    let path = dir.join("sous-dossier").join("strava.json");
    assert_eq!(Tokens::load(&path).unwrap(), None);
    let t = Tokens {
        access_token: "a".into(),
        refresh_token: "r".into(),
        expires_at: 99,
        athlete_name: "Jeannie Longo".into(),
    };
    t.save(&path).unwrap();
    assert_eq!(Tokens::load(&path).unwrap(), Some(t.clone()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    Tokens::forget(&path).unwrap();
    Tokens::forget(&path).unwrap();
    assert_eq!(Tokens::load(&path).unwrap(), None);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn multipart_body_structure() {
    let fit = [0x0E, 0x10, 0x00, 0xFF, b'\r', b'\n'];
    let body = multipart_body(
        "XYZ",
        &[("data_type", "fit"), ("name", "Col du Galibier")],
        ("file", "activity.fit", &fit),
    );
    let mut expected =
        b"--XYZ\r\nContent-Disposition: form-data; name=\"data_type\"\r\n\r\nfit\r\n\
--XYZ\r\nContent-Disposition: form-data; name=\"name\"\r\n\r\nCol du Galibier\r\n\
--XYZ\r\nContent-Disposition: form-data; name=\"file\"; filename=\"activity.fit\"\r\n\
Content-Type: application/octet-stream\r\n\r\n"
            .to_vec();
    expected.extend_from_slice(&fit);
    expected.extend_from_slice(b"\r\n--XYZ--\r\n");
    assert_eq!(body, expected);
}

#[test]
fn upload_responses_are_parsed() {
    let processing = r#"{"id":16486788,"id_str":"16486788","external_id":"velotrace-1.fit",
        "error":null,"status":"Your activity is still being processed.","activity_id":null}"#;
    assert_eq!(
        parse_upload_response(processing).unwrap(),
        (16_486_788, UploadStatus::Processing)
    );
    let ready = r#"{"id":16486788,"id_str":"16486788","external_id":"velotrace-1.fit",
        "error":null,"status":"Your activity is ready.","activity_id":12345678987}"#;
    assert_eq!(
        parse_upload_response(ready).unwrap(),
        (16_486_788, UploadStatus::Ready(12_345_678_987))
    );
    let bad = r#"{"id":16486789,"external_id":null,"error":"Improperly formatted data.",
        "status":"There was an error processing your activity.","activity_id":null}"#;
    assert_eq!(
        parse_upload_response(bad).unwrap().1,
        UploadStatus::Error("Improperly formatted data.".into())
    );
}

#[test]
fn duplicate_maps_to_ready() {
    let dup = r#"{"id":16486790,"external_id":"velotrace-1.fit",
        "error":"activity.fit duplicate of activity 9876543210",
        "status":"There was an error processing your activity.","activity_id":null}"#;
    assert_eq!(
        parse_upload_response(dup).unwrap().1,
        UploadStatus::Ready(9_876_543_210)
    );
    assert_eq!(
        duplicate_of("Duplicate of <a href='/activities/555'>555</a>"),
        Some(555)
    );
    assert_eq!(duplicate_of("duplicate of something"), None);
    assert_eq!(duplicate_of("Improperly formatted data."), None);
}

#[test]
fn request_line_parsing() {
    assert_eq!(
        parse_request("GET /?state=&code=abc123&scope=read,activity:write HTTP/1.1\r\n\r\n"),
        Callback::Code {
            code: "abc123".into(),
            scope: Some("read,activity:write".into())
        }
    );
    assert_eq!(
        parse_request("GET /?state=&error=access_denied HTTP/1.1\r\n"),
        Callback::Error("access_denied".into())
    );
    assert_eq!(
        parse_request("GET /favicon.ico HTTP/1.1\r\n"),
        Callback::Other
    );
    assert_eq!(parse_request("POST /?code=x HTTP/1.1\r\n"), Callback::Other);
    assert_eq!(parse_request(""), Callback::Other);
}

/// Envoie une requête au serveur local et renvoie la réponse brute.
fn browser(port: u16, target: &'static str) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            s,
            "GET {target} HTTP/1.1\r\nHost: localhost:{port}\r\nAccept: text/html\r\n\r\n"
        )
        .unwrap();
        let mut resp = String::new();
        s.read_to_string(&mut resp).unwrap();
        resp
    })
}

#[test]
fn listener_receives_the_code() {
    let listener = CodeListener::bind().unwrap();
    let port = listener.port();
    assert_ne!(port, 0);
    // Le navigateur demande souvent l'icône du site avant ou après : ignorée.
    let mut favicon = TcpStream::connect(("127.0.0.1", port)).unwrap();
    favicon
        .write_all(b"GET /favicon.ico HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .unwrap();
    let page = browser(port, "/?state=&code=d1e2f3%2Bx&scope=read,activity:write");
    let code = listener.wait(Duration::from_secs(5)).unwrap();
    assert_eq!(code, "d1e2f3+x");
    let resp = page.join().unwrap();
    assert!(resp.starts_with("HTTP/1.1 200 OK\r\n"), "{resp}");
    assert!(resp.contains("Connexion à Strava réussie"));
    let mut resp = String::new();
    favicon.read_to_string(&mut resp).unwrap();
    assert!(resp.starts_with("HTTP/1.1 404"), "{resp}");
}

#[test]
fn listener_reports_refusal_and_missing_scope() {
    let listener = CodeListener::bind().unwrap();
    let page = browser(listener.port(), "/?state=&error=access_denied");
    assert!(matches!(
        listener.wait(Duration::from_secs(5)),
        Err(StravaError::AuthorizationDenied)
    ));
    assert!(page.join().unwrap().contains("refusée"));

    let page = browser(listener.port(), "/?state=&code=abc&scope=read");
    assert!(matches!(
        listener.wait(Duration::from_secs(5)),
        Err(StravaError::MissingScope)
    ));
    page.join().unwrap();
}

#[test]
fn listener_times_out() {
    let listener = CodeListener::bind().unwrap();
    assert!(matches!(
        listener.wait(Duration::from_millis(120)),
        Err(StravaError::Timeout)
    ));
}
