//! Profil sur disque et accès champ par champ depuis l'écran des paramètres.
//!
//! Godot désigne un champ par son chemin TOML (« rider.weight_kg »,
//! « bike.cassette ») ; la valeur passe par le TOML du profil, ce qui garde
//! la validation de serde et les valeurs par défaut des champs absents.

use godot::prelude::*;
use std::path::PathBuf;
use velo_core::config::Profile;

/// Fichier du profil : `~/.config/velotrace/profile.toml` sous Linux,
/// `%APPDATA%\velotrace\profile.toml` sous Windows.
pub fn path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("velotrace")
        .join("profile.toml")
}

/// Lit le profil ; à défaut (premier lancement, fichier illisible), celui de Martin.
pub fn load() -> Profile {
    let path = path();
    match std::fs::read_to_string(&path) {
        Ok(text) => match Profile::from_toml(&text) {
            Ok(p) => p,
            Err(e) => {
                godot_warn!("profil illisible ({}) : {e}", path.display());
                Profile::martin()
            }
        },
        Err(_) => Profile::martin(),
    }
}

pub fn save(profile: &Profile) -> Result<(), String> {
    let path = path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{} : {e}", dir.display()))?;
    }
    let text = profile.to_toml().map_err(|e| e.to_string())?;
    // Écriture atomique : un profil à moitié écrit serait perdu au prochain lancement.
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{} : {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("{} : {e}", path.display()))
}

fn lookup<'a>(root: &'a toml::Value, key: &str) -> Option<&'a toml::Value> {
    key.split('.').try_fold(root, |v, k| v.get(k))
}

fn to_variant(v: &toml::Value) -> Variant {
    match v {
        toml::Value::String(s) => GString::from(s.as_str()).to_variant(),
        toml::Value::Integer(i) => i.to_variant(),
        toml::Value::Float(f) => f.to_variant(),
        toml::Value::Boolean(b) => b.to_variant(),
        toml::Value::Array(a) => {
            let mut out = VarArray::new();
            for x in a {
                out.push(&to_variant(x));
            }
            out.to_variant()
        }
        toml::Value::Datetime(d) => GString::from(d.to_string().as_str()).to_variant(),
        toml::Value::Table(_) => Variant::nil(),
    }
}

/// Valeur d'un champ (`nil` s'il est absent, comme une FTP inconnue).
pub fn get(profile: &Profile, key: &str) -> Variant {
    let Ok(root) = toml::Value::try_from(profile) else {
        return Variant::nil();
    };
    lookup(&root, key).map_or(Variant::nil(), to_variant)
}

fn number(v: &Variant) -> Option<f64> {
    match v.get_type() {
        VariantType::INT => v.try_to::<i64>().ok().map(|i| i as f64),
        VariantType::FLOAT => v.try_to::<f64>().ok(),
        VariantType::STRING => v
            .try_to::<GString>()
            .ok()
            .and_then(|s| s.to_string().trim().replace(',', ".").parse().ok()),
        _ => None,
    }
}

/// Liste d'entiers : tableau Godot ou texte « 36, 52 ».
fn integers(v: &Variant) -> Option<Vec<i64>> {
    if let Ok(a) = v.try_to::<VarArray>() {
        return a.iter_shared().map(|x| number(&x).map(|f| f.round() as i64)).collect();
    }
    if let Ok(a) = v.try_to::<PackedInt32Array>() {
        return Some(a.as_slice().iter().map(|x| *x as i64).collect());
    }
    let s = v.try_to::<GString>().ok()?.to_string();
    s.split([',', ';', ' ', '-', '/'])
        .filter(|x| !x.trim().is_empty())
        .map(|x| x.trim().parse::<i64>().ok())
        .collect()
}

/// Convertit `value` au type du champ actuel (ou deviné pour un champ absent).
fn to_toml(current: Option<&toml::Value>, key: &str, value: &Variant) -> Result<Option<toml::Value>, String> {
    let bad = || format!("valeur invalide pour {key}");
    Ok(Some(match current {
        Some(toml::Value::Float(_)) => toml::Value::Float(number(value).ok_or_else(bad)?),
        Some(toml::Value::Integer(_)) => toml::Value::Integer(number(value).ok_or_else(bad)?.round() as i64),
        Some(toml::Value::Boolean(_)) => toml::Value::Boolean(value.try_to::<bool>().map_err(|_| bad())?),
        Some(toml::Value::String(_)) => toml::Value::String(value.to_string()),
        Some(toml::Value::Array(_)) => toml::Value::Array(
            integers(value).ok_or_else(bad)?.into_iter().map(toml::Value::Integer).collect(),
        ),
        // Champ facultatif absent (FTP inconnue) : un entier, 0 ou vide pour l'effacer.
        None => {
            if value.is_nil() {
                return Ok(None);
            }
            let n = number(value).ok_or_else(bad)?;
            if n <= 0.0 {
                return Ok(None);
            }
            toml::Value::Integer(n.round() as i64)
        }
        Some(_) => return Err(bad()),
    }))
}

/// Profil modifié, ou la raison du refus (en français).
pub fn set(profile: &Profile, key: &str, value: &Variant) -> Result<Profile, String> {
    let mut root = toml::Value::try_from(profile).map_err(|e| e.to_string())?;
    let (section, field) = key.rsplit_once('.').ok_or_else(|| format!("champ inconnu : {key}"))?;
    let table = section
        .split('.')
        .try_fold(&mut root, |v, k| v.get_mut(k))
        .and_then(|v| v.as_table_mut())
        .ok_or_else(|| format!("champ inconnu : {key}"))?;
    // Les seuls champs facultatifs absents du TOML sont des entiers (FTP).
    if !table.contains_key(field) && key != "rider.ftp_w" {
        return Err(format!("champ inconnu : {key}"));
    }
    let current = if key == "rider.ftp_w" { None } else { table.get(field) };
    let new = to_toml(current, key, value)?;
    match new {
        Some(v) => table.insert(field.to_string(), v),
        None => table.remove(field),
    };
    let mut p: Profile = root.try_into().map_err(|e: toml::de::Error| e.message().to_string())?;
    p.bike.chainrings.sort_unstable();
    p.bike.cassette.sort_unstable();
    validate(&p)?;
    Ok(p)
}

fn check(ok: bool, msg: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(msg.to_string())
    }
}

/// Bornes raisonnables : évite une sortie absurde après une faute de frappe.
pub fn validate(p: &Profile) -> Result<(), String> {
    check((25.0..=200.0).contains(&p.rider.weight_kg), "le poids du cycliste doit être entre 25 et 200 kg")?;
    check(p.rider.ftp_w.is_none_or(|f| (50..=600).contains(&f)), "la FTP doit être entre 50 et 600 W")?;
    check((3.0..=40.0).contains(&p.bike.weight_kg), "le poids du vélo doit être entre 3 et 40 kg")?;
    check(
        !p.bike.chainrings.is_empty() && p.bike.chainrings.len() <= 3 && p.bike.chainrings.iter().all(|c| (20..=70).contains(c)),
        "plateaux : 1 à 3 valeurs entre 20 et 70 dents",
    )?;
    check(
        p.bike.cassette.len() >= 2 && p.bike.cassette.len() <= 14 && p.bike.cassette.iter().all(|c| (9..=60).contains(c)),
        "cassette : 2 à 14 pignons entre 9 et 60 dents",
    )?;
    check((1.5..=2.6).contains(&p.bike.wheel_circumference_m), "la circonférence de roue doit être entre 1,5 et 2,6 m")?;
    check((0.1..=1.0).contains(&p.bike.cda_m2), "le CdA doit être entre 0,1 et 1 m²")?;
    check((0.001..=0.03).contains(&p.bike.crr), "le Crr doit être entre 0,001 et 0,03")?;
    check((0.8..=1.0).contains(&p.bike.drivetrain_efficiency), "le rendement doit être entre 80 et 100 %")?;
    check((20..=70).contains(&p.trainer.physical_chainring), "plateau physique : entre 20 et 70 dents")?;
    check((9..=60).contains(&p.trainer.physical_cog), "pignon physique : entre 9 et 60 dents")?;
    check((0.0..=1.0).contains(&p.trainer.grade_difficulty), "la difficulté des pentes doit être entre 0 et 100 %")?;
    check(
        (-0.4..=0.0).contains(&p.trainer.min_grade) && (0.0..=0.4).contains(&p.trainer.max_grade),
        "pentes limites : minimum entre -40 et 0 %, maximum entre 0 et 40 %",
    )?;
    check(p.app.camera <= 3, "caméra inconnue")?;
    check(["reel", "cache", "simple"].contains(&p.app.decor.as_str()), "décor inconnu")?;
    check(p.scenery.quality <= 2, "qualité inconnue")?;
    check(["cadence", "puissance"].contains(&p.app.test_rider.as_str()), "cycliste virtuel inconnu")?;
    Ok(())
}
