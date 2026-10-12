//! Séances paramétrées : blocs à puissance cible (en fraction de FTP),
//! rampes et passages libres, import des fichiers Zwift `.zwo` et lecture
//! pendant une sortie.

use serde::{Deserialize, Serialize};

/// Consigne d'un bloc, en fraction de FTP (0,9 = 90 % de FTP).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Target {
    /// Pas de consigne : le cycliste roule comme il veut (pente simulée).
    Free,
    /// Puissance constante.
    Power { ftp: f64 },
    /// Puissance qui évolue linéairement du début à la fin du bloc.
    Ramp { from: f64, to: f64 },
}

/// Bloc d'une séance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub duration_s: f64,
    pub target: Target,
    /// Cadence conseillée.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cadence_rpm: Option<f64>,
    #[serde(default)]
    pub label: String,
}

impl Step {
    pub fn power(duration_s: f64, ftp: f64) -> Self {
        Self {
            duration_s,
            target: Target::Power { ftp },
            cadence_rpm: None,
            label: String::new(),
        }
    }

    pub fn ramp(duration_s: f64, from: f64, to: f64) -> Self {
        Self {
            duration_s,
            target: Target::Ramp { from, to },
            cadence_rpm: None,
            label: String::new(),
        }
    }

    pub fn free(duration_s: f64) -> Self {
        Self {
            duration_s,
            target: Target::Free,
            cadence_rpm: None,
            label: String::new(),
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn cadence(mut self, rpm: f64) -> Self {
        self.cadence_rpm = Some(rpm);
        self
    }

    /// Fraction de FTP visée `elapsed_s` secondes après le début du bloc.
    pub fn fraction_at(&self, elapsed_s: f64) -> Option<f64> {
        match self.target {
            Target::Free => None,
            Target::Power { ftp } => Some(ftp),
            Target::Ramp { from, to } => {
                let k = if self.duration_s > 0.0 {
                    (elapsed_s / self.duration_s).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                Some(from + (to - from) * k)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum WorkoutError {
    #[error("fichier .zwo sans bloc <workout>")]
    NoWorkout,
    #[error("la séance ne contient aucun bloc")]
    Empty,
    #[error("attribut {attribute} invalide dans <{element}> : « {value} »")]
    BadAttribute {
        element: String,
        attribute: String,
        value: String,
    },
    #[error("XML mal formé : {0}")]
    Malformed(&'static str),
}

/// Séance complète.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workout {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub steps: Vec<Step>,
}

impl Workout {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            steps: Vec::new(),
        }
    }

    /// Durée totale (s).
    pub fn duration_s(&self) -> f64 {
        self.steps.iter().map(|s| s.duration_s).sum()
    }

    /// Instant de début du bloc `index` (ou fin de séance au-delà).
    pub fn step_start_s(&self, index: usize) -> f64 {
        self.steps.iter().take(index).map(|s| s.duration_s).sum()
    }

    /// Bloc en cours à `t_s` : `(indice, temps écoulé dans le bloc, temps restant)`.
    /// `None` avant le départ ou après la fin.
    pub fn step_at(&self, t_s: f64) -> Option<(usize, f64, f64)> {
        if t_s < 0.0 {
            return None;
        }
        let mut start = 0.0;
        for (i, s) in self.steps.iter().enumerate() {
            let end = start + s.duration_s;
            if t_s < end {
                return Some((i, t_s - start, end - t_s));
            }
            start = end;
        }
        None
    }

    /// Fraction de FTP visée à `t_s` ; `None` en bloc libre ou après la fin.
    pub fn target_fraction_at(&self, t_s: f64) -> Option<f64> {
        let (i, elapsed, _) = self.step_at(t_s)?;
        self.steps[i].fraction_at(elapsed)
    }

    // --- Construction rapide (préréglages) ---

    pub fn step(mut self, step: Step) -> Self {
        self.steps.push(step);
        self
    }

    /// Échauffement progressif.
    pub fn warmup(self, duration_s: f64, from: f64, to: f64) -> Self {
        self.step(Step::ramp(duration_s, from, to).label("Échauffement"))
    }

    /// Retour au calme dégressif.
    pub fn cooldown(self, duration_s: f64, from: f64, to: f64) -> Self {
        self.step(Step::ramp(duration_s, from, to).label("Retour au calme"))
    }

    /// Bloc à puissance constante.
    pub fn steady(self, duration_s: f64, ftp: f64, label: &str) -> Self {
        self.step(Step::power(duration_s, ftp).label(label))
    }

    /// `repeat` répétitions d'un effort suivi d'une récupération.
    pub fn intervals(
        mut self,
        repeat: usize,
        (on_s, on_ftp): (f64, f64),
        (off_s, off_ftp): (f64, f64),
        cadence_rpm: Option<f64>,
    ) -> Self {
        for i in 1..=repeat {
            let mut on = Step::power(on_s, on_ftp).label(format!("Effort {i}/{repeat}"));
            on.cadence_rpm = cadence_rpm;
            self.steps.push(on);
            if off_s > 0.0 {
                self.steps
                    .push(Step::power(off_s, off_ftp).label("Récupération"));
            }
        }
        self
    }

    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    pub fn from_toml(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }

    /// Importe une séance Zwift (`.zwo`).
    ///
    /// Blocs reconnus : `SteadyState`, `Warmup`, `Cooldown`, `Ramp`,
    /// `IntervalsT`, `FreeRide` et `MaxEffort` (en bloc libre). Les autres
    /// éléments (`textevent`…) sont ignorés.
    pub fn from_zwo(xml: &str) -> Result<Workout, WorkoutError> {
        let mut out = Workout::new("Séance importée", "");
        let mut in_workout = false;
        let mut seen_workout = false;
        // Champ texte en cours de lecture (<name> ou <description>).
        let mut capture: Option<(String, String)> = None;
        for event in XmlEvents::new(xml) {
            match event? {
                XmlEvent::Start(tag) => {
                    let name = tag.name.to_ascii_lowercase();
                    if name == "workout" {
                        seen_workout = true;
                        in_workout = !tag.self_closing;
                    } else if in_workout {
                        zwo_element(&tag, &mut out.steps)?;
                    } else if (name == "name" || name == "description") && !tag.self_closing {
                        capture = Some((name, String::new()));
                    }
                }
                XmlEvent::Text(text) => {
                    if let Some((_, buf)) = capture.as_mut() {
                        buf.push_str(&text);
                    }
                }
                XmlEvent::End(name) => {
                    let name = name.to_ascii_lowercase();
                    if name == "workout" {
                        in_workout = false;
                    }
                    if let Some((field, buf)) = capture.take() {
                        if field == name {
                            let value = buf.trim().to_string();
                            if field == "name" {
                                if !value.is_empty() {
                                    out.name = value;
                                }
                            } else {
                                out.description = value;
                            }
                        } else {
                            capture = Some((field, buf));
                        }
                    }
                }
            }
        }
        if !seen_workout {
            return Err(WorkoutError::NoWorkout);
        }
        if out.steps.is_empty() {
            return Err(WorkoutError::Empty);
        }
        Ok(out)
    }
}

/// Transforme un élément du bloc `<workout>` en blocs de séance.
fn zwo_element(tag: &Tag, steps: &mut Vec<Step>) -> Result<(), WorkoutError> {
    let kind = tag.name.to_ascii_lowercase();
    let num = |key: &str| -> Result<Option<f64>, WorkoutError> {
        match tag.attr(key) {
            None => Ok(None),
            Some(v) => v
                .trim()
                .parse::<f64>()
                .map(Some)
                .map_err(|_| WorkoutError::BadAttribute {
                    element: tag.name.clone(),
                    attribute: key.to_string(),
                    value: v.to_string(),
                }),
        }
    };
    let duration = num("duration")?.unwrap_or(0.0);
    let cadence = num("cadence")?;
    let low_high = || -> Result<Option<(f64, f64)>, WorkoutError> {
        Ok(match (num("powerlow")?, num("powerhigh")?) {
            (Some(l), Some(h)) => Some((l, h)),
            (Some(p), None) | (None, Some(p)) => Some((p, p)),
            (None, None) => num("power")?.map(|p| (p, p)),
        })
    };
    let mut push = |step: Step| {
        if step.duration_s > 0.0 {
            steps.push(step);
        }
    };
    let with_cadence = |mut s: Step, c: Option<f64>| {
        s.cadence_rpm = c;
        s
    };
    match kind.as_str() {
        "steadystate" => {
            let target = match low_high()? {
                Some((l, h)) => Target::Power { ftp: 0.5 * (l + h) },
                None => Target::Free,
            };
            push(with_cadence(
                Step {
                    duration_s: duration,
                    target,
                    cadence_rpm: None,
                    label: "Bloc".into(),
                },
                cadence,
            ));
        }
        "warmup" | "cooldown" | "ramp" => {
            let (label, (from, to)) = match (kind.as_str(), low_high()?) {
                (_, None) => {
                    push(with_cadence(Step::free(duration).label("Libre"), cadence));
                    return Ok(());
                }
                // Zwift lit parfois PowerLow/PowerHigh à l'envers selon les
                // éditeurs : l'échauffement monte, le retour au calme descend.
                ("warmup", Some((l, h))) => ("Échauffement", (l.min(h), l.max(h))),
                ("cooldown", Some((l, h))) => ("Retour au calme", (l.max(h), l.min(h))),
                (_, Some(lh)) => ("Rampe", lh),
            };
            push(with_cadence(
                Step::ramp(duration, from, to).label(label),
                cadence,
            ));
        }
        "intervalst" => {
            let repeat = num("repeat")?.unwrap_or(1.0).max(1.0).round() as usize;
            let on_s = num("onduration")?.unwrap_or(0.0);
            let off_s = num("offduration")?.unwrap_or(0.0);
            let on = match (num("poweronlow")?, num("poweronhigh")?, num("onpower")?) {
                (Some(l), Some(h), _) => ramp_or_power(l, h),
                (_, _, Some(p)) | (Some(p), None, None) | (None, Some(p), None) => {
                    Target::Power { ftp: p }
                }
                _ => Target::Free,
            };
            let off = match (num("powerofflow")?, num("poweroffhigh")?, num("offpower")?) {
                (Some(l), Some(h), _) => ramp_or_power(l, h),
                (_, _, Some(p)) | (Some(p), None, None) | (None, Some(p), None) => {
                    Target::Power { ftp: p }
                }
                _ => Target::Free,
            };
            let off_cadence = num("cadenceresting")?;
            for i in 1..=repeat {
                push(Step {
                    duration_s: on_s,
                    target: on,
                    cadence_rpm: cadence,
                    label: format!("Effort {i}/{repeat}"),
                });
                push(Step {
                    duration_s: off_s,
                    target: off,
                    cadence_rpm: off_cadence,
                    label: "Récupération".into(),
                });
            }
        }
        "freeride" => push(with_cadence(Step::free(duration).label("Libre"), cadence)),
        "maxeffort" => push(with_cadence(
            Step::free(duration).label("Effort maximal"),
            cadence,
        )),
        _ => {}
    }
    Ok(())
}

/// Rampe, ou puissance constante si les deux bornes sont égales.
fn ramp_or_power(from: f64, to: f64) -> Target {
    if from == to {
        Target::Power { ftp: from }
    } else {
        Target::Ramp { from, to }
    }
}

// --- Lecture XML minimale (juste ce qu'il faut pour les .zwo) ---

struct Tag {
    name: String,
    attrs: Vec<(String, String)>,
    self_closing: bool,
}

impl Tag {
    /// Valeur d'un attribut, sans tenir compte de la casse du nom.
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }
}

enum XmlEvent {
    Start(Tag),
    End(String),
    Text(String),
}

struct XmlEvents<'a> {
    rest: &'a str,
}

impl<'a> XmlEvents<'a> {
    fn new(xml: &'a str) -> Self {
        Self {
            rest: xml.trim_start_matches('\u{feff}'),
        }
    }

    /// Saute jusqu'après `end` ; erreur si absent.
    fn skip_past(&mut self, end: &str) -> Result<&'a str, WorkoutError> {
        let i = self
            .rest
            .find(end)
            .ok_or(WorkoutError::Malformed("balise non terminée"))?;
        let inner = &self.rest[..i];
        self.rest = &self.rest[i + end.len()..];
        Ok(inner)
    }

    fn next_event(&mut self) -> Result<Option<XmlEvent>, WorkoutError> {
        loop {
            if self.rest.is_empty() {
                return Ok(None);
            }
            if !self.rest.starts_with('<') {
                let end = self.rest.find('<').unwrap_or(self.rest.len());
                let text = &self.rest[..end];
                self.rest = &self.rest[end..];
                return Ok(Some(XmlEvent::Text(unescape(text))));
            }
            if let Some(r) = self.rest.strip_prefix("<!--") {
                self.rest = r;
                self.skip_past("-->")?;
            } else if let Some(r) = self.rest.strip_prefix("<![CDATA[") {
                self.rest = r;
                let text = self.skip_past("]]>")?;
                return Ok(Some(XmlEvent::Text(text.to_string())));
            } else if self.rest.starts_with("<?") || self.rest.starts_with("<!") {
                self.skip_past(">")?;
            } else if let Some(r) = self.rest.strip_prefix("</") {
                self.rest = r;
                let name = self.skip_past(">")?.trim().to_string();
                return Ok(Some(XmlEvent::End(name)));
            } else {
                self.rest = &self.rest[1..];
                return self.start_tag().map(|t| Some(XmlEvent::Start(t)));
            }
        }
    }

    fn start_tag(&mut self) -> Result<Tag, WorkoutError> {
        let s = self.rest;
        let is_name_end = |c: char| c.is_whitespace() || c == '>' || c == '/';
        let name_end = s.find(is_name_end).unwrap_or(s.len());
        let name = s[..name_end].to_string();
        if name.is_empty() {
            return Err(WorkoutError::Malformed("balise sans nom"));
        }
        let mut rest = &s[name_end..];
        let mut attrs = Vec::new();
        loop {
            rest = rest.trim_start();
            if let Some(r) = rest.strip_prefix("/>") {
                self.rest = r;
                return Ok(Tag {
                    name,
                    attrs,
                    self_closing: true,
                });
            }
            if let Some(r) = rest.strip_prefix('>') {
                self.rest = r;
                return Ok(Tag {
                    name,
                    attrs,
                    self_closing: false,
                });
            }
            let key_end = rest
                .find(|c: char| c == '=' || c.is_whitespace() || c == '>' || c == '/')
                .ok_or(WorkoutError::Malformed("balise non terminée"))?;
            if key_end == 0 {
                return Err(WorkoutError::Malformed("attribut sans nom"));
            }
            let key = rest[..key_end].to_string();
            rest = rest[key_end..].trim_start();
            let Some(r) = rest.strip_prefix('=') else {
                // Attribut sans valeur (HTML) : toléré.
                attrs.push((key, String::new()));
                continue;
            };
            rest = r.trim_start();
            let quote = rest
                .chars()
                .next()
                .filter(|c| *c == '"' || *c == '\'')
                .ok_or(WorkoutError::Malformed("valeur d'attribut sans guillemets"))?;
            rest = &rest[1..];
            let end = rest
                .find(quote)
                .ok_or(WorkoutError::Malformed("guillemet non fermé"))?;
            attrs.push((key, unescape(&rest[..end])));
            rest = &rest[end + 1..];
        }
    }
}

impl Iterator for XmlEvents<'_> {
    type Item = Result<XmlEvent, WorkoutError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_event() {
            Ok(e) => e.map(Ok),
            Err(e) => {
                self.rest = "";
                Some(Err(e))
            }
        }
    }
}

/// Remplace les entités XML usuelles et numériques.
fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let decoded = rest.find(';').and_then(|end| {
            let entity = &rest[1..end];
            let c = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => {
                    let code = if let Some(hex) = entity
                        .strip_prefix("#x")
                        .or_else(|| entity.strip_prefix("#X"))
                    {
                        u32::from_str_radix(hex, 16).ok()
                    } else {
                        entity.strip_prefix('#').and_then(|d| d.parse().ok())
                    };
                    code.and_then(char::from_u32)
                }
            };
            c.map(|c| (c, end))
        });
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

// --- Préréglages ---

/// Séances proposées d'office, toutes avec échauffement et retour au calme.
pub fn presets() -> Vec<Workout> {
    let min = 60.0;
    vec![
        Workout::new(
            "Endurance 45 min",
            "Sortie d'endurance fondamentale avec des variations de cadence.",
        )
        .warmup(5.0 * min, 0.45, 0.65)
        .steady(10.0 * min, 0.65, "Endurance")
        .step(
            Step::power(5.0 * min, 0.70)
                .cadence(100.0)
                .label("Vélocité"),
        )
        .steady(10.0 * min, 0.68, "Endurance")
        .step(Step::power(5.0 * min, 0.72).cadence(70.0).label("Force"))
        .steady(5.0 * min, 0.65, "Endurance")
        .cooldown(5.0 * min, 0.60, 0.40),
        Workout::new(
            "Sweet spot 3 × 10 min",
            "Trois blocs de 10 min juste sous le seuil (90 % de FTP), 5 min de récupération.",
        )
        .warmup(10.0 * min, 0.45, 0.75)
        .intervals(3, (10.0 * min, 0.90), (5.0 * min, 0.55), Some(90.0))
        .cooldown(5.0 * min, 0.55, 0.40),
        Workout::new(
            "Seuil 2 × 20 min",
            "Deux blocs de 20 min au seuil (97 % de FTP), 5 min de récupération.",
        )
        .warmup(10.0 * min, 0.45, 0.75)
        .intervals(2, (20.0 * min, 0.97), (5.0 * min, 0.55), Some(90.0))
        .cooldown(5.0 * min, 0.55, 0.40),
        Workout::new(
            "VO2max 5 × 3 min",
            "Cinq efforts de 3 min à 115 % de FTP, récupération égale.",
        )
        .warmup(10.0 * min, 0.45, 0.75)
        .intervals(5, (3.0 * min, 1.15), (3.0 * min, 0.50), Some(95.0))
        .cooldown(10.0 * min, 0.55, 0.40),
        pyramid(),
        Workout::new(
            "Test FTP 20 min",
            "Échauffement avec trois accélérations, puis 20 min à fond mais \
             régulier : la FTP vaut 95 % de la puissance moyenne.",
        )
        .warmup(10.0 * min, 0.45, 0.75)
        .intervals(3, (1.0 * min, 1.00), (1.0 * min, 0.55), Some(100.0))
        .steady(4.0 * min, 0.55, "Récupération")
        .step(Step::free(20.0 * min).label("Test 20 min : effort maximal régulier"))
        .cooldown(10.0 * min, 0.55, 0.40),
    ]
}

fn pyramid() -> Workout {
    let mut w = Workout::new(
        "Pyramide 1-2-3-4-3-2-1",
        "Efforts de 1 à 4 min puis retour à 1 min, d'autant plus intenses \
         qu'ils sont courts ; 2 min de récupération entre chaque.",
    )
    .warmup(600.0, 0.45, 0.75);
    let blocks = [
        (1.0, 1.20),
        (2.0, 1.10),
        (3.0, 1.05),
        (4.0, 1.00),
        (3.0, 1.05),
        (2.0, 1.10),
        (1.0, 1.20),
    ];
    for (i, (minutes, ftp)) in blocks.iter().enumerate() {
        w = w
            .steady(
                minutes * 60.0,
                *ftp,
                &format!("Palier {}/{}", i + 1, blocks.len()),
            )
            .steady(120.0, 0.50, "Récupération");
    }
    w.cooldown(600.0, 0.55, 0.40)
}

// --- Lecture pendant la sortie ---

/// Déroule une séance pendant une sortie.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkoutPlayer {
    pub workout: Workout,
    /// Temps écoulé dans la séance (s).
    pub elapsed_s: f64,
    pub ftp_w: f64,
    /// Multiplicateur des consignes (1,0 = séance telle quelle), réglable en pause.
    pub intensity: f64,
    /// Mode ERG : le home trainer impose la puissance ; sinon la pente simulée reste.
    pub erg: bool,
}

/// Plage du multiplicateur d'intensité.
pub const INTENSITY_RANGE: (f64, f64) = (0.5, 1.5);

/// État de la séance pour l'affichage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerState {
    pub step_index: usize,
    pub step_count: usize,
    pub step_elapsed_s: f64,
    pub step_remaining_s: f64,
    pub target_w: Option<f64>,
    /// Consigne au début du bloc suivant.
    pub next_target_w: Option<f64>,
    pub cadence_rpm: Option<f64>,
    pub total_elapsed_s: f64,
    pub total_s: f64,
}

impl WorkoutPlayer {
    pub fn new(workout: Workout, ftp_w: f64) -> Self {
        Self {
            workout,
            elapsed_s: 0.0,
            ftp_w,
            intensity: 1.0,
            erg: true,
        }
    }

    /// Avance de `dt` secondes.
    pub fn advance(&mut self, dt: f64) {
        self.elapsed_s = (self.elapsed_s + dt.max(0.0)).min(self.workout.duration_s());
    }

    pub fn set_intensity(&mut self, intensity: f64) {
        self.intensity = intensity.clamp(INTENSITY_RANGE.0, INTENSITY_RANGE.1);
    }

    fn watts(&self, fraction: f64) -> f64 {
        (fraction * self.ftp_w * self.intensity).round()
    }

    /// Puissance cible (W) ; `None` en bloc libre ou une fois la séance finie.
    pub fn target_w(&self) -> Option<f64> {
        self.workout
            .target_fraction_at(self.elapsed_s)
            .map(|f| self.watts(f))
    }

    /// Bloc en cours.
    pub fn current_step(&self) -> Option<&Step> {
        self.workout
            .step_at(self.elapsed_s)
            .map(|(i, _, _)| &self.workout.steps[i])
    }

    /// Passe directement au bloc suivant.
    pub fn skip_step(&mut self) {
        if let Some((i, _, _)) = self.workout.step_at(self.elapsed_s) {
            self.elapsed_s = self.workout.step_start_s(i + 1);
        }
    }

    pub fn finished(&self) -> bool {
        self.elapsed_s >= self.workout.duration_s()
    }

    pub fn state(&self) -> PlayerState {
        let step_count = self.workout.steps.len();
        let total_s = self.workout.duration_s();
        let (step_index, step_elapsed_s, step_remaining_s) = self
            .workout
            .step_at(self.elapsed_s)
            .unwrap_or((step_count, 0.0, 0.0));
        let step = self.workout.steps.get(step_index);
        PlayerState {
            step_index,
            step_count,
            step_elapsed_s,
            step_remaining_s,
            target_w: self.target_w(),
            next_target_w: self
                .workout
                .steps
                .get(step_index + 1)
                .and_then(|s| s.fraction_at(0.0))
                .map(|f| self.watts(f)),
            cadence_rpm: step.and_then(|s| s.cadence_rpm),
            total_elapsed_s: self.elapsed_s,
            total_s,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<workout_file>
    <author>Moi</author>
    <name>Essai &amp; rampes</name>
    <description><![CDATA[Petite séance <de test>]]></description>
    <sportType>bike</sportType>
    <tags><tag name="test"/></tags>
    <workout>
        <!-- commentaire <SteadyState Duration="999" Power="9"/> -->
        <Warmup Duration="300" PowerLow="0.40" PowerHigh="0.70"/>
        <SteadyState Duration="120" Power="0.85" Cadence="95">
            <textevent timeoffset="10" message="Allez !"/>
        </SteadyState>
        <IntervalsT Repeat="2" OnDuration="60" OffDuration="30" OnPower="1.2" OffPower="0.5" Cadence="100" CadenceResting="85"/>
        <IntervalsT Repeat="1" OnDuration="40" OffDuration="20" PowerOnLow="0.9" PowerOnHigh="1.1" PowerOffLow="0.5" PowerOffHigh="0.5"/>
        <Ramp Duration="100" PowerLow="0.6" PowerHigh="1.0"/>
        <FreeRide Duration="60" FlatRoad="1"/>
        <MaxEffort Duration="20"/>
        <Inconnu Duration="500" Power="2"/>
        <Cooldown Duration="200" PowerLow="0.30" PowerHigh="0.60"/>
    </workout>
</workout_file>
"#;

    #[test]
    fn zwo_import_reads_every_element() {
        let w = Workout::from_zwo(SAMPLE).unwrap();
        assert_eq!(w.name, "Essai & rampes");
        assert_eq!(w.description, "Petite séance <de test>");
        // 1 + 1 + 4 + 2 + 1 + 1 + 1 + 1 blocs.
        assert_eq!(w.steps.len(), 12);
        assert_eq!(
            w.steps[0].target,
            Target::Ramp {
                from: 0.40,
                to: 0.70
            }
        );
        assert_eq!(w.steps[1].target, Target::Power { ftp: 0.85 });
        assert_eq!(w.steps[1].cadence_rpm, Some(95.0));
        assert_eq!(w.steps[2].target, Target::Power { ftp: 1.2 });
        assert_eq!(w.steps[2].cadence_rpm, Some(100.0));
        assert_eq!(w.steps[2].label, "Effort 1/2");
        assert_eq!(w.steps[3].target, Target::Power { ftp: 0.5 });
        assert_eq!(w.steps[3].duration_s, 30.0);
        assert_eq!(w.steps[3].cadence_rpm, Some(85.0));
        assert_eq!(w.steps[4].duration_s, 60.0);
        assert_eq!(w.steps[6].target, Target::Ramp { from: 0.9, to: 1.1 });
        assert_eq!(w.steps[7].target, Target::Power { ftp: 0.5 });
        assert_eq!(w.steps[8].target, Target::Ramp { from: 0.6, to: 1.0 });
        assert_eq!(w.steps[9].target, Target::Free);
        assert_eq!(w.steps[10].target, Target::Free);
        assert_eq!(w.steps[10].label, "Effort maximal");
        // Le retour au calme descend quel que soit l'ordre des attributs.
        assert_eq!(
            w.steps[11].target,
            Target::Ramp {
                from: 0.60,
                to: 0.30
            }
        );
        assert_eq!(
            w.duration_s(),
            300.0 + 120.0 + 180.0 + 60.0 + 100.0 + 60.0 + 20.0 + 200.0
        );
    }

    #[test]
    fn zwo_errors() {
        assert_eq!(
            Workout::from_zwo("<workout_file><name>x</name></workout_file>"),
            Err(WorkoutError::NoWorkout)
        );
        assert_eq!(
            Workout::from_zwo("<workout_file><workout></workout></workout_file>"),
            Err(WorkoutError::Empty)
        );
        assert!(matches!(
            Workout::from_zwo(r#"<workout><SteadyState Duration="abc" Power="1"/></workout>"#),
            Err(WorkoutError::BadAttribute { .. })
        ));
        assert!(matches!(
            Workout::from_zwo(r#"<workout><SteadyState Duration="60"#),
            Err(WorkoutError::Malformed(_))
        ));
    }

    #[test]
    fn target_and_step_at_given_times() {
        let w = Workout::new("t", "")
            .warmup(100.0, 0.5, 0.7)
            .steady(60.0, 1.0, "Bloc")
            .step(Step::free(30.0))
            .cooldown(50.0, 0.6, 0.4);
        assert_eq!(w.duration_s(), 240.0);
        assert_eq!(w.target_fraction_at(-1.0), None);
        assert!((w.target_fraction_at(0.0).unwrap() - 0.5).abs() < 1e-12);
        // Rampe : interpolation linéaire.
        assert!((w.target_fraction_at(50.0).unwrap() - 0.6).abs() < 1e-12);
        assert!((w.target_fraction_at(75.0).unwrap() - 0.65).abs() < 1e-12);
        assert_eq!(w.target_fraction_at(100.0), Some(1.0));
        assert_eq!(w.target_fraction_at(170.0), None);
        assert!((w.target_fraction_at(215.0).unwrap() - 0.5).abs() < 1e-12);
        assert_eq!(w.target_fraction_at(240.0), None);
        assert_eq!(w.step_at(130.0), Some((1, 30.0, 30.0)));
        assert_eq!(w.step_at(160.0), Some((2, 0.0, 30.0)));
        assert_eq!(w.step_at(1000.0), None);
    }

    #[test]
    fn player_targets_skip_and_state() {
        let w = Workout::new("t", "")
            .warmup(100.0, 0.5, 0.7)
            .step(Step::power(60.0, 1.0).cadence(95.0))
            .step(Step::free(30.0));
        let mut p = WorkoutPlayer::new(w, 250.0);
        assert_eq!(p.target_w(), Some(125.0));
        p.advance(50.0);
        assert_eq!(p.target_w(), Some(150.0));
        let s = p.state();
        assert_eq!(s.step_index, 0);
        assert_eq!(s.step_count, 3);
        assert_eq!(s.step_remaining_s, 50.0);
        assert_eq!(s.next_target_w, Some(250.0));
        p.set_intensity(1.1);
        assert_eq!(p.target_w(), Some(165.0));
        p.set_intensity(3.0);
        assert_eq!(p.intensity, 1.5);
        p.set_intensity(1.0);
        p.skip_step();
        let s = p.state();
        assert_eq!((s.step_index, s.step_elapsed_s), (1, 0.0));
        assert_eq!(s.target_w, Some(250.0));
        assert_eq!(s.cadence_rpm, Some(95.0));
        assert_eq!(s.next_target_w, None);
        p.skip_step();
        assert_eq!(p.target_w(), None);
        assert!(!p.finished());
        p.advance(1000.0);
        assert!(p.finished());
        let s = p.state();
        assert_eq!(s.step_index, 3);
        assert_eq!(s.total_elapsed_s, s.total_s);
        p.skip_step();
        assert_eq!(p.elapsed_s, 190.0);
    }

    #[test]
    fn presets_match_their_names() {
        let all = presets();
        assert!(all.len() >= 6);
        for w in &all {
            assert!(!w.steps.is_empty(), "{}", w.name);
            assert!(!w.description.is_empty(), "{}", w.name);
            assert_eq!(w.steps[0].label, "Échauffement", "{}", w.name);
            assert_eq!(w.steps.last().unwrap().label, "Retour au calme");
            assert!(w.steps.iter().all(|s| s.duration_s > 0.0));
            // Durées rondes, en minutes.
            assert_eq!(w.duration_s() % 60.0, 0.0, "{}", w.name);
        }
        let find = |prefix: &str| all.iter().find(|w| w.name.starts_with(prefix)).unwrap();
        let count = |w: &Workout, secs: f64, min: f64, max: f64| {
            w.steps
                .iter()
                .filter(|s| {
                    s.duration_s == secs
                        && matches!(s.target, Target::Power { ftp } if (min..=max).contains(&ftp))
                })
                .count()
        };
        assert_eq!(find("Endurance").duration_s(), 45.0 * 60.0);
        assert_eq!(count(find("Sweet spot"), 600.0, 0.88, 0.94), 3);
        assert_eq!(count(find("Seuil"), 1200.0, 0.95, 1.05), 2);
        assert_eq!(count(find("VO2max"), 180.0, 1.10, 1.30), 5);
        let pyr: Vec<f64> = find("Pyramide")
            .steps
            .iter()
            .filter(|s| s.label.starts_with("Palier"))
            .map(|s| s.duration_s / 60.0)
            .collect();
        assert_eq!(pyr, [1.0, 2.0, 3.0, 4.0, 3.0, 2.0, 1.0]);
        let test = find("Test FTP");
        assert!(test
            .steps
            .iter()
            .any(|s| s.target == Target::Free && s.duration_s == 1200.0));
    }

    #[test]
    fn toml_round_trip() {
        for w in presets() {
            let text = w.to_toml().unwrap();
            assert_eq!(Workout::from_toml(&text).unwrap(), w);
        }
        let zwo = Workout::from_zwo(SAMPLE).unwrap();
        assert_eq!(Workout::from_toml(&zwo.to_toml().unwrap()).unwrap(), zwo);
        // Fichier écrit à la main : libellé et description facultatifs.
        let hand = r#"
            name = "Maison"
            [[steps]]
            duration_s = 60
            target = { type = "power", ftp = 0.8 }
            [[steps]]
            duration_s = 30.0
            target = { type = "free" }
            cadence_rpm = 90.0
        "#;
        let w = Workout::from_toml(hand).unwrap();
        assert_eq!(w.steps.len(), 2);
        assert_eq!(w.steps[0].target, Target::Power { ftp: 0.8 });
        assert_eq!(w.steps[1].cadence_rpm, Some(90.0));
    }
}
