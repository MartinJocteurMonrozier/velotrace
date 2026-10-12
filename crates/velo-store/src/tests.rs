use super::*;
use velo_core::record::summarize;

/// Lundi 1er janvier 2024, 00:00 UTC.
const MONDAY: i64 = 1_704_067_200;
const DAY: i64 = SECONDS_PER_DAY;

fn samples(watts: &[f64]) -> Vec<Sample> {
    watts
        .iter()
        .enumerate()
        .map(|(i, w)| Sample {
            t_s: i as f64,
            distance_m: i as f64 * 8.0,
            lat: 45.188_529_123_456_7 + i as f64 * 1e-7,
            lon: 5.724_524_987_654_3 - i as f64 * 1e-7,
            elevation_m: 200.0 + (i % 50) as f64 * 0.5,
            speed_ms: 8.0,
            power_w: *w,
            cadence_rpm: 90.0,
            heart_rate: if i % 3 == 0 { None } else { Some(140) },
            grade: 0.031_25,
        })
        .collect()
}

fn ride(started_unix: i64, watts: f64, seconds: usize) -> NewRide {
    let samples = samples(&vec![watts; seconds]);
    NewRide {
        started_unix,
        name: format!("Sortie à {watts} W"),
        route_name: "Col de Porte".into(),
        mode: RideMode::Libre,
        ftp_w: 250.0,
        rider_kg: 72.0,
        summary: summarize(&samples, 250.0),
        samples,
    }
}

#[test]
fn schema_is_up_to_date() {
    let store = Store::in_memory().unwrap();
    assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
}

#[test]
fn reopening_a_file_keeps_rides() {
    let dir = std::env::temp_dir().join(format!("velo-store-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("velotrace.db");
    let _ = std::fs::remove_file(&path);
    {
        let mut store = Store::open(&path).unwrap();
        store.save_ride(&ride(MONDAY, 200.0, 120)).unwrap();
    }
    let store = Store::open(&path).unwrap();
    assert_eq!(store.ride_count().unwrap(), 1);
    assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
    drop(store);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn newer_database_is_refused() {
    let conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
        .unwrap();
    assert!(matches!(Store::init(conn), Err(StoreError::TooNew(_))));
}

#[test]
fn save_list_and_delete() {
    let mut store = Store::in_memory().unwrap();
    let mut first = ride(MONDAY, 200.0, 600);
    first.mode = RideMode::Seance;
    let a = store.save_ride(&first).unwrap();
    let b = store.save_ride(&ride(MONDAY + DAY, 220.0, 300)).unwrap();
    let c = store
        .save_ride(&ride(MONDAY + 2 * DAY, 180.0, 300))
        .unwrap();
    assert_eq!(store.ride_count().unwrap(), 3);

    let list = store.list_rides(10, 0).unwrap();
    assert_eq!(list.iter().map(|r| r.id).collect::<Vec<_>>(), [c, b, a]);
    let page = store.list_rides(1, 1).unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].id, b);

    let row = store.ride(a).unwrap().unwrap();
    assert_eq!(row.mode, RideMode::Seance);
    assert_eq!(row.route_name, "Col de Porte");
    assert_eq!(row.summary, first.summary);
    assert_eq!(row.strava_activity_id, None);
    assert_eq!(row.notes, "");

    store.set_strava_activity(a, 12_345_678_901).unwrap();
    store.set_notes(a, "Jambes lourdes").unwrap();
    let row = store.ride(a).unwrap().unwrap();
    assert_eq!(row.strava_activity_id, Some(12_345_678_901));
    assert_eq!(row.notes, "Jambes lourdes");

    assert!(store.delete_ride(b).unwrap());
    assert!(!store.delete_ride(b).unwrap());
    assert_eq!(store.ride_count().unwrap(), 2);
    assert!(store.ride(b).unwrap().is_none());
    assert!(store.samples(b).unwrap().is_empty());
    assert!(store.ride_curve(b).unwrap().is_empty());
}

#[test]
fn samples_round_trip() {
    let mut store = Store::in_memory().unwrap();
    let r = ride(MONDAY, 250.0, 500);
    let id = store.save_ride(&r).unwrap();
    let back = store.samples(id).unwrap();
    // Les valeurs de test tiennent exactement en f32 sauf les coordonnées,
    // stockées en f64.
    assert_eq!(back, r.samples);
    assert_eq!(back[0].heart_rate, None);
    assert_eq!(back[1].heart_rate, Some(140));
}

#[test]
fn corrupt_blob_is_reported() {
    let mut blob = encode_samples(&samples(&[100.0; 10]));
    blob.pop();
    assert!(matches!(
        decode_samples(&blob),
        Err(StoreError::CorruptSamples(_))
    ));
    assert!(decode_samples(b"nope").is_err());
    assert!(decode_samples(&encode_samples(&[])).unwrap().is_empty());
}

#[test]
fn best_curve_takes_the_max_across_rides() {
    let mut store = Store::in_memory().unwrap();
    // Sortie courte et intense, puis sortie longue et modérée.
    let mut punchy = vec![150.0; 300];
    for w in punchy.iter_mut().skip(100).take(10) {
        *w = 800.0;
    }
    let mut short = ride(MONDAY, 0.0, 0);
    short.samples = samples(&punchy);
    let a = store.save_ride(&short).unwrap();
    let b = store.save_ride(&ride(MONDAY + DAY, 260.0, 1500)).unwrap();

    let curve = store.ride_curve(a).unwrap();
    assert_eq!(curve.first(), Some(&(1, 800.0)));
    assert_eq!(curve.last().unwrap().0, 300);

    let best = store.best_curve(None).unwrap();
    let at = |d: u32| *best.iter().find(|p| p.duration_s == d).unwrap();
    assert_eq!(at(1).ride_id, a);
    assert_eq!(at(1).watts, 800.0);
    assert_eq!(at(10).ride_id, a);
    assert_eq!(at(300).ride_id, b);
    assert_eq!(at(300).started_unix, MONDAY + DAY);
    assert_eq!(at(1200).ride_id, b);
    assert!(best.windows(2).all(|w| w[0].duration_s < w[1].duration_s));

    // Depuis le deuxième jour, seule la sortie longue compte.
    let recent = store.best_curve(Some(MONDAY + DAY)).unwrap();
    assert!(recent.iter().all(|p| p.ride_id == b));
    assert_eq!(recent[0].watts, 260.0);
    assert!(store.best_curve(Some(MONDAY + 2 * DAY)).unwrap().is_empty());
}

#[test]
fn weekly_totals_bucket_by_monday() {
    assert_eq!(week_start(MONDAY), MONDAY);
    assert_eq!(week_start(MONDAY + 7 * DAY - 1), MONDAY);
    assert_eq!(week_start(MONDAY - 1), MONDAY - 7 * DAY);

    let mut store = Store::in_memory().unwrap();
    // Dimanche soir de la semaine 1, lundi matin et mercredi de la semaine 2.
    store
        .save_ride(&ride(MONDAY + 7 * DAY - 60, 200.0, 60))
        .unwrap();
    store
        .save_ride(&ride(MONDAY + 7 * DAY + 3600, 200.0, 120))
        .unwrap();
    store
        .save_ride(&ride(MONDAY + 9 * DAY, 200.0, 180))
        .unwrap();
    // Trop ancienne pour la fenêtre demandée.
    store
        .save_ride(&ride(MONDAY - 30 * DAY, 200.0, 60))
        .unwrap();

    let now = MONDAY + 21 * DAY + 5 * 3600; // semaine 4
    let weeks = store.weekly_totals(4, now).unwrap();
    assert_eq!(weeks.len(), 4);
    let starts: Vec<i64> = weeks.iter().map(|w| w.week_start_unix).collect();
    assert_eq!(
        starts,
        [
            MONDAY,
            MONDAY + 7 * DAY,
            MONDAY + 14 * DAY,
            MONDAY + 21 * DAY
        ]
    );
    assert_eq!(weeks[0].rides, 1);
    assert_eq!(weeks[0].duration_s, 60.0);
    assert_eq!(weeks[1].rides, 2);
    assert_eq!(weeks[1].duration_s, 300.0);
    assert!((weeks[1].distance_m - (119.0 + 179.0) * 8.0).abs() < 1e-9);
    assert!(weeks[1].tss > 0.0 && weeks[1].work_kj > 0.0);
    assert_eq!(
        weeks[2],
        WeekTotals {
            week_start_unix: MONDAY + 14 * DAY,
            ..Default::default()
        }
    );
    assert_eq!(weeks[3].rides, 0);
    assert!(store.weekly_totals(0, now).unwrap().is_empty());
}

#[test]
fn fitness_grows_with_training() {
    let mut store = Store::in_memory().unwrap();
    // Une heure au seuil (≈ 100 TSS) chaque jour pendant 30 jours.
    for d in 0..30 {
        store
            .save_ride(&ride(MONDAY + d * DAY + 18 * 3600, 250.0, 3600))
            .unwrap();
    }
    let now = MONDAY + 29 * DAY + 20 * 3600;
    let days = store.fitness(60, now).unwrap();
    assert_eq!(days.len(), 60);
    assert_eq!(days.last().unwrap().day_unix, MONDAY + 29 * DAY);
    // Les jours d'avant la première sortie sont vides.
    assert!(days[..30].iter().all(|d| d.tss == 0.0 && d.ctl == 0.0));
    let training = &days[30..];
    assert!((training[0].tss - 100.0).abs() < 0.5);
    assert!((training[0].ctl - 100.0 / 42.0).abs() < 0.05);
    assert!((training[0].atl - 100.0 / 7.0).abs() < 0.1);
    assert!(training.windows(2).all(|w| w[1].ctl > w[0].ctl));
    // La fatigue monte plus vite que la condition : fraîcheur négative.
    let last = training.last().unwrap();
    assert!(last.atl > last.ctl);
    assert!(last.tsb < 0.0);

    // Après une semaine de repos, la fatigue retombe et la fraîcheur revient.
    let rested = store.fitness(1, now + 10 * DAY).unwrap()[0];
    assert_eq!(rested.tss, 0.0);
    assert!(rested.atl < last.atl && rested.ctl < last.ctl);
    assert!(rested.tsb > 0.0);
    // Les 3 derniers jours seulement : le calcul part quand même du début.
    let short = store.fitness(3, now).unwrap();
    assert_eq!(short.as_slice(), &days[57..]);
}

#[test]
fn records_across_rides() {
    let mut store = Store::in_memory().unwrap();
    assert_eq!(store.records().unwrap(), Records::default());
    let mut climb = ride(MONDAY, 180.0, 900);
    climb.summary.elevation_gain_m = 1200.0;
    store.save_ride(&climb).unwrap();
    store.save_ride(&ride(MONDAY + DAY, 240.0, 1800)).unwrap();
    store
        .save_ride(&ride(MONDAY + 2 * DAY, 280.0, 1250))
        .unwrap();
    let rec = store.records().unwrap();
    assert_eq!(rec.longest_s, 1800.0);
    assert!((rec.farthest_m - 1799.0 * 8.0).abs() < 1e-9);
    assert_eq!(rec.most_elevation_m, 1200.0);
    assert_eq!(rec.best_20min_w, Some(280.0));
    assert!((rec.estimated_ftp_w.unwrap() - 266.0).abs() < 1e-9);
}

#[test]
fn mode_names() {
    for m in RideMode::ALL {
        assert_eq!(RideMode::parse(m.as_str()), m);
    }
    assert_eq!(RideMode::parse("inconnu"), RideMode::Libre);
}
