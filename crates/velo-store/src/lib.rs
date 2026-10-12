//! Carnet de sorties de velotrace : une base SQLite locale qui garde chaque
//! sortie (bilan, échantillons, courbe de puissance) pour le suivi au long
//! cours — records, totaux hebdomadaires, forme (CTL/ATL/TSB).
//!
//! Le schéma évolue par migrations successives numérotées dans
//! `PRAGMA user_version` : une base ancienne est mise à niveau à l'ouverture,
//! une base plus récente que l'application est refusée.

mod samples;

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension, Row};
use velo_core::record::{estimated_ftp, power_curve, Sample, Summary};

pub use samples::{decode_samples, encode_samples};

/// Version du schéma écrite par cette version de l'application.
pub const SCHEMA_VERSION: i32 = 1;

/// Migrations : l'entrée `i` fait passer la base de la version `i` à `i + 1`.
const MIGRATIONS: &[&str] = &[
    // 0 → 1 : schéma initial.
    "
    CREATE TABLE rides (
        id                 INTEGER PRIMARY KEY AUTOINCREMENT,
        started_unix       INTEGER NOT NULL,
        name               TEXT    NOT NULL,
        route_name         TEXT    NOT NULL,
        mode               TEXT    NOT NULL,
        duration_s         REAL    NOT NULL,
        distance_m         REAL    NOT NULL,
        elevation_gain_m   REAL    NOT NULL,
        avg_speed          REAL    NOT NULL,
        max_speed          REAL    NOT NULL,
        avg_power          REAL    NOT NULL,
        max_power          REAL    NOT NULL,
        np                 REAL    NOT NULL,
        if_                REAL    NOT NULL,
        tss                REAL    NOT NULL,
        work_kj            REAL    NOT NULL,
        avg_cadence        REAL    NOT NULL,
        avg_hr             REAL,
        max_hr             INTEGER,
        ftp_w              REAL    NOT NULL,
        rider_kg           REAL    NOT NULL,
        strava_activity_id INTEGER,
        notes              TEXT    NOT NULL DEFAULT ''
    );
    CREATE INDEX rides_started ON rides (started_unix);

    CREATE TABLE samples (
        ride_id INTEGER PRIMARY KEY REFERENCES rides (id) ON DELETE CASCADE,
        count   INTEGER NOT NULL,
        data    BLOB    NOT NULL
    );

    CREATE TABLE power_curve (
        ride_id    INTEGER NOT NULL REFERENCES rides (id) ON DELETE CASCADE,
        duration_s INTEGER NOT NULL,
        watts      REAL    NOT NULL,
        PRIMARY KEY (ride_id, duration_s)
    ) WITHOUT ROWID;
    CREATE INDEX power_curve_best ON power_curve (duration_s, watts DESC);
    ",
];

const SECONDS_PER_DAY: i64 = 86_400;
const SECONDS_PER_WEEK: i64 = 7 * SECONDS_PER_DAY;

/// Erreurs du carnet de sorties.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("base de données des sorties : {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("dossier de la base de données : {0}")]
    Io(#[from] std::io::Error),
    #[error("aucun dossier de données utilisateur sur ce système")]
    NoDataDir,
    #[error("la base des sorties (version {0}) vient d'une version plus récente de velotrace")]
    TooNew(i32),
    #[error("échantillons de la sortie illisibles : {0}")]
    CorruptSamples(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// Type de sortie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RideMode {
    /// Roulage libre sur un parcours.
    #[default]
    Libre,
    /// Séance structurée (intervalles).
    Seance,
    /// Course contre des adversaires.
    Adversaires,
    /// Test (FTP, rampe…).
    Test,
}

impl RideMode {
    pub const ALL: [RideMode; 4] = [
        RideMode::Libre,
        RideMode::Seance,
        RideMode::Adversaires,
        RideMode::Test,
    ];

    /// Nom stocké dans la base.
    pub fn as_str(self) -> &'static str {
        match self {
            RideMode::Libre => "libre",
            RideMode::Seance => "seance",
            RideMode::Adversaires => "adversaires",
            RideMode::Test => "test",
        }
    }

    /// Mode d'après son nom stocké ; un nom inconnu donne `Libre`.
    pub fn parse(s: &str) -> RideMode {
        RideMode::ALL
            .into_iter()
            .find(|m| m.as_str() == s)
            .unwrap_or_default()
    }
}

/// Sortie à enregistrer.
#[derive(Debug, Clone, Default)]
pub struct NewRide {
    /// Début, en secondes depuis le 1er janvier 1970 (UTC).
    pub started_unix: i64,
    pub name: String,
    pub route_name: String,
    pub mode: RideMode,
    /// FTP du cycliste au moment de la sortie (W).
    pub ftp_w: f64,
    /// Masse du cycliste (kg).
    pub rider_kg: f64,
    pub summary: Summary,
    pub samples: Vec<Sample>,
}

/// Sortie enregistrée, sans ses échantillons.
#[derive(Debug, Clone, PartialEq)]
pub struct RideRow {
    pub id: i64,
    pub started_unix: i64,
    pub name: String,
    pub route_name: String,
    pub mode: RideMode,
    pub ftp_w: f64,
    pub rider_kg: f64,
    pub summary: Summary,
    /// Identifiant de l'activité Strava, une fois la sortie envoyée.
    pub strava_activity_id: Option<u64>,
    pub notes: String,
}

/// Meilleure puissance pour une durée, avec la sortie qui la détient.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurvePoint {
    pub duration_s: u32,
    pub watts: f64,
    pub ride_id: i64,
    pub started_unix: i64,
}

/// Totaux d'une semaine (du lundi 00:00 UTC au lundi suivant).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WeekTotals {
    pub week_start_unix: i64,
    pub rides: u32,
    pub duration_s: f64,
    pub distance_m: f64,
    pub elevation_m: f64,
    pub tss: f64,
    pub work_kj: f64,
}

/// Forme d'un jour (UTC).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FitnessDay {
    /// Début du jour (00:00 UTC).
    pub day_unix: i64,
    /// Charge du jour (somme des TSS des sorties).
    pub tss: f64,
    /// Condition physique (CTL) : moyenne exponentielle sur 42 jours.
    pub ctl: f64,
    /// Fatigue (ATL) : moyenne exponentielle sur 7 jours.
    pub atl: f64,
    /// Fraîcheur (TSB) : CTL − ATL de la veille, avant la charge du jour.
    pub tsb: f64,
}

/// Records personnels, toutes sorties confondues.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Records {
    pub longest_s: f64,
    pub farthest_m: f64,
    pub most_elevation_m: f64,
    /// Meilleure puissance moyenne sur 20 min.
    pub best_20min_w: Option<f64>,
    /// FTP estimée : 95 % du meilleur 20 min.
    pub estimated_ftp_w: Option<f64>,
}

/// Carnet de sorties ouvert.
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Ouvre (ou crée) la base au chemin donné et la met à niveau.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    /// Chemin par défaut : ~/.local/share/velotrace/velotrace.db (Linux),
    /// %APPDATA%\velotrace\velotrace.db (Windows).
    pub fn default_path() -> Option<PathBuf> {
        dirs::data_dir().map(|d| d.join("velotrace").join("velotrace.db"))
    }

    /// Ouvre la base au chemin par défaut, en créant son dossier au besoin.
    pub fn open_default() -> Result<Self> {
        let path = Self::default_path().ok_or(StoreError::NoDataDir)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::open(path)
    }

    /// Base en mémoire, vide à chaque ouverture (tests).
    pub fn in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", true)?;
        migrate(&mut conn)?;
        Ok(Self { conn })
    }

    /// Version du schéma de la base ouverte.
    pub fn schema_version(&self) -> Result<i32> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))?)
    }

    /// Enregistre une sortie avec ses échantillons et sa courbe de puissance ;
    /// renvoie son identifiant.
    pub fn save_ride(&mut self, ride: &NewRide) -> Result<i64> {
        let s = &ride.summary;
        let curve = power_curve(&ride.samples);
        let blob = encode_samples(&ride.samples);
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO rides (
                started_unix, name, route_name, mode, duration_s, distance_m,
                elevation_gain_m, avg_speed, max_speed, avg_power, max_power, np,
                if_, tss, work_kj, avg_cadence, avg_hr, max_hr, ftp_w, rider_kg
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                      ?15, ?16, ?17, ?18, ?19, ?20)",
            params![
                ride.started_unix,
                ride.name,
                ride.route_name,
                ride.mode.as_str(),
                s.duration_s,
                s.distance_m,
                s.elevation_gain_m,
                s.avg_speed_ms,
                s.max_speed_ms,
                s.avg_power_w,
                s.max_power_w,
                s.normalized_power_w,
                s.intensity_factor,
                s.tss,
                s.work_kj,
                s.avg_cadence_rpm,
                s.avg_heart_rate,
                s.max_heart_rate,
                ride.ftp_w,
                ride.rider_kg,
            ],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO samples (ride_id, count, data) VALUES (?1, ?2, ?3)",
            params![id, ride.samples.len() as i64, blob],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO power_curve (ride_id, duration_s, watts) VALUES (?1, ?2, ?3)",
            )?;
            for (d, w) in &curve {
                stmt.execute(params![id, d, w])?;
            }
        }
        tx.commit()?;
        Ok(id)
    }

    /// Sorties de la plus récente à la plus ancienne, sans échantillons.
    pub fn list_rides(&self, limit: usize, offset: usize) -> Result<Vec<RideRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {RIDE_COLUMNS} FROM rides
             ORDER BY started_unix DESC, id DESC LIMIT ?1 OFFSET ?2"
        ))?;
        let rows = stmt.query_map(
            params![limit.min(i64::MAX as usize) as i64, offset as i64],
            ride_from_row,
        )?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Nombre de sorties enregistrées.
    pub fn ride_count(&self) -> Result<usize> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM rides", [], |r| r.get(0))?;
        Ok(n as usize)
    }

    /// Une sortie, sans ses échantillons.
    pub fn ride(&self, id: i64) -> Result<Option<RideRow>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {RIDE_COLUMNS} FROM rides WHERE id = ?1"),
                [id],
                ride_from_row,
            )
            .optional()?)
    }

    /// Échantillons d'une sortie (vide si la sortie n'existe pas).
    pub fn samples(&self, id: i64) -> Result<Vec<Sample>> {
        let blob: Option<Vec<u8>> = self
            .conn
            .query_row("SELECT data FROM samples WHERE ride_id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?;
        match blob {
            Some(b) => decode_samples(&b),
            None => Ok(Vec::new()),
        }
    }

    /// Supprime une sortie et tout ce qui s'y rattache ; renvoie faux si elle
    /// n'existait pas.
    pub fn delete_ride(&mut self, id: i64) -> Result<bool> {
        let n = self.conn.execute("DELETE FROM rides WHERE id = ?1", [id])?;
        Ok(n > 0)
    }

    /// Note l'activité Strava correspondant à la sortie.
    pub fn set_strava_activity(&mut self, id: i64, activity_id: u64) -> Result<()> {
        self.conn.execute(
            "UPDATE rides SET strava_activity_id = ?2 WHERE id = ?1",
            params![id, activity_id as i64],
        )?;
        Ok(())
    }

    /// Remplace les notes libres de la sortie.
    pub fn set_notes(&mut self, id: i64, notes: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE rides SET notes = ?2 WHERE id = ?1",
            params![id, notes],
        )?;
        Ok(())
    }

    /// Courbe de puissance d'une sortie : `(durée en s, watts)`, durées
    /// croissantes.
    pub fn ride_curve(&self, id: i64) -> Result<Vec<(u32, f64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT duration_s, watts FROM power_curve WHERE ride_id = ?1 ORDER BY duration_s",
        )?;
        let rows = stmt.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Meilleure puissance par durée, toutes sorties confondues (ou depuis
    /// `since_unix`), durées croissantes. À égalité, la sortie la plus
    /// ancienne garde le record.
    pub fn best_curve(&self, since_unix: Option<i64>) -> Result<Vec<CurvePoint>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.duration_s, p.watts, p.ride_id, r.started_unix
             FROM power_curve p JOIN rides r ON r.id = p.ride_id
             WHERE r.started_unix >= ?1
             ORDER BY p.duration_s, p.watts DESC, r.started_unix, r.id",
        )?;
        let rows = stmt.query_map([since_unix.unwrap_or(i64::MIN)], |r| {
            Ok(CurvePoint {
                duration_s: r.get(0)?,
                watts: r.get(1)?,
                ride_id: r.get(2)?,
                started_unix: r.get(3)?,
            })
        })?;
        let mut best: Vec<CurvePoint> = Vec::new();
        for p in rows {
            let p = p?;
            if best.last().map(|b| b.duration_s) != Some(p.duration_s) {
                best.push(p);
            }
        }
        Ok(best)
    }

    /// Totaux des `weeks` dernières semaines, de la plus ancienne à celle
    /// contenant `now_unix`, semaines vides comprises.
    pub fn weekly_totals(&self, weeks: usize, now_unix: i64) -> Result<Vec<WeekTotals>> {
        if weeks == 0 {
            return Ok(Vec::new());
        }
        let current = week_start(now_unix);
        let first = current - (weeks as i64 - 1) * SECONDS_PER_WEEK;
        let mut out: Vec<WeekTotals> = (0..weeks as i64)
            .map(|i| WeekTotals {
                week_start_unix: first + i * SECONDS_PER_WEEK,
                ..Default::default()
            })
            .collect();
        let mut stmt = self.conn.prepare(
            "SELECT started_unix, duration_s, distance_m, elevation_gain_m, tss, work_kj
             FROM rides WHERE started_unix >= ?1 AND started_unix < ?2",
        )?;
        let mut rows = stmt.query(params![first, current + SECONDS_PER_WEEK])?;
        while let Some(r) = rows.next()? {
            let started: i64 = r.get(0)?;
            let w = &mut out[((started - first) / SECONDS_PER_WEEK) as usize];
            w.rides += 1;
            w.duration_s += r.get::<_, f64>(1)?;
            w.distance_m += r.get::<_, f64>(2)?;
            w.elevation_m += r.get::<_, f64>(3)?;
            w.tss += r.get::<_, f64>(4)?;
            w.work_kj += r.get::<_, f64>(5)?;
        }
        Ok(out)
    }

    /// Forme des `days` derniers jours (le dernier est celui de `now_unix`),
    /// du plus ancien au plus récent. Les moyennes exponentielles partent de
    /// zéro au jour de la première sortie : `ctl += (tss − ctl) / 42`,
    /// `atl += (tss − atl) / 7`.
    pub fn fitness(&self, days: usize, now_unix: i64) -> Result<Vec<FitnessDay>> {
        if days == 0 {
            return Ok(Vec::new());
        }
        let today = now_unix.div_euclid(SECONDS_PER_DAY);
        let first_shown = today - (days as i64 - 1);
        let mut daily: Vec<(i64, f64)> = Vec::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT started_unix, tss FROM rides WHERE started_unix < ?1
                 ORDER BY started_unix",
            )?;
            let mut rows = stmt.query([(today + 1) * SECONDS_PER_DAY])?;
            while let Some(r) = rows.next()? {
                let day = r.get::<_, i64>(0)?.div_euclid(SECONDS_PER_DAY);
                let tss: f64 = r.get(1)?;
                match daily.last_mut() {
                    Some((d, t)) if *d == day => *t += tss,
                    _ => daily.push((day, tss)),
                }
            }
        }
        let start = daily
            .first()
            .map_or(first_shown, |(d, _)| (*d).min(first_shown));
        let mut out = Vec::with_capacity(days);
        let (mut ctl, mut atl) = (0.0, 0.0);
        let mut next = daily.iter().peekable();
        for day in start..=today {
            let tss = match next.peek() {
                Some((d, t)) if *d == day => {
                    next.next();
                    *t
                }
                _ => 0.0,
            };
            let tsb = ctl - atl;
            ctl += (tss - ctl) / 42.0;
            atl += (tss - atl) / 7.0;
            if day >= first_shown {
                out.push(FitnessDay {
                    day_unix: day * SECONDS_PER_DAY,
                    tss,
                    ctl,
                    atl,
                    tsb,
                });
            }
        }
        Ok(out)
    }

    /// Records personnels.
    pub fn records(&self) -> Result<Records> {
        let (longest_s, farthest_m, most_elevation_m) = self.conn.query_row(
            "SELECT COALESCE(MAX(duration_s), 0), COALESCE(MAX(distance_m), 0),
                    COALESCE(MAX(elevation_gain_m), 0)
             FROM rides",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let best_20min_w: Option<f64> = self.conn.query_row(
            "SELECT MAX(watts) FROM power_curve WHERE duration_s = 1200",
            [],
            |r| r.get(0),
        )?;
        Ok(Records {
            longest_s,
            farthest_m,
            most_elevation_m,
            best_20min_w,
            estimated_ftp_w: best_20min_w.and_then(|w| estimated_ftp(&[(1200, w)])),
        })
    }
}

/// Début de la semaine (lundi 00:00 UTC) contenant l'instant donné.
pub fn week_start(unix: i64) -> i64 {
    let day = unix.div_euclid(SECONDS_PER_DAY);
    // Le 1er janvier 1970 était un jeudi : lundi = 0.
    let weekday = (day + 3).rem_euclid(7);
    (day - weekday) * SECONDS_PER_DAY
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(StoreError::TooNew(version));
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version.max(0) as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i as i32 + 1)?;
        tx.commit()?;
    }
    Ok(())
}

const RIDE_COLUMNS: &str = "id, started_unix, name, route_name, mode, duration_s, distance_m,
    elevation_gain_m, avg_speed, max_speed, avg_power, max_power, np, if_, tss, work_kj,
    avg_cadence, avg_hr, max_hr, ftp_w, rider_kg, strava_activity_id, notes";

fn ride_from_row(r: &Row) -> rusqlite::Result<RideRow> {
    Ok(RideRow {
        id: r.get(0)?,
        started_unix: r.get(1)?,
        name: r.get(2)?,
        route_name: r.get(3)?,
        mode: RideMode::parse(&r.get::<_, String>(4)?),
        summary: Summary {
            duration_s: r.get(5)?,
            distance_m: r.get(6)?,
            elevation_gain_m: r.get(7)?,
            avg_speed_ms: r.get(8)?,
            max_speed_ms: r.get(9)?,
            avg_power_w: r.get(10)?,
            max_power_w: r.get(11)?,
            normalized_power_w: r.get(12)?,
            intensity_factor: r.get(13)?,
            tss: r.get(14)?,
            work_kj: r.get(15)?,
            avg_cadence_rpm: r.get(16)?,
            avg_heart_rate: r.get(17)?,
            max_heart_rate: r.get(18)?,
        },
        ftp_w: r.get(19)?,
        rider_kg: r.get(20)?,
        strava_activity_id: r.get::<_, Option<i64>>(21)?.map(|v| v as u64),
        notes: r.get(22)?,
    })
}

#[cfg(test)]
mod tests;
