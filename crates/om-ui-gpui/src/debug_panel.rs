//! `--debug-panel` (Todo 26a): device, latency and CPU figures, the router's unknown-id count and a
//! raw list of the control surface's current values. The row builder is pure and tested headless;
//! the window only draws the rows.

use om_engine::DjEngine;

/// What the window shows about the audio device. The binary fills it in from its device.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceStatus {
    pub name: String,
    pub sample_rate: f64,
    pub buffer_size: usize,
    pub round_trip_ms: f64,
    pub callback_avg_us: f64,
    pub callback_max_us: f64,
    pub budget_us: f64,
    pub cpu_load: f64,
    pub xruns: u64,
    pub callbacks: u64,
}

/// Lines of the device section, `(label, value)`.
pub fn device_rows(status: &DeviceStatus) -> Vec<(String, String)> {
    vec![
        ("device".to_owned(), status.name.clone()),
        (
            "sample rate".to_owned(),
            format!("{:.0} Hz", status.sample_rate),
        ),
        (
            "buffer".to_owned(),
            format!("{} frames", status.buffer_size),
        ),
        (
            "round trip".to_owned(),
            format!("{:.2} ms", status.round_trip_ms),
        ),
        (
            "callback avg / max".to_owned(),
            format!(
                "{:.0} / {:.0} us (budget {:.0} us)",
                status.callback_avg_us, status.callback_max_us, status.budget_us
            ),
        ),
        (
            "cpu load".to_owned(),
            format!("{:.1} %", status.cpu_load * 100.0),
        ),
        ("xruns".to_owned(), status.xruns.to_string()),
        ("callbacks".to_owned(), status.callbacks.to_string()),
    ]
}

/// Every control-surface value the models read, as `(id, value)` in router id form.
pub fn parameter_rows(engine: &DjEngine, unknown: usize) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let mixer = engine.mixer();
    rows.push(("router.unknown".to_owned(), unknown.to_string()));
    for deck in 0..engine.num_decks() {
        let d = engine.deck(deck);
        let p = format!("deck.{deck}");
        rows.push((format!("{p}.rate"), format!("{:.4}", d.rate())));
        rows.push((format!("{p}.playing"), d.is_playing().to_string()));
        rows.push((format!("{p}.position"), d.position_frames().to_string()));
        rows.push((format!("{p}.length"), d.length_frames().to_string()));
        rows.push((format!("{p}.looping"), d.is_looping().to_string()));
        rows.push((format!("{p}.keylock"), d.is_keylock().to_string()));
        rows.push((
            format!("sync.enabled.{deck}"),
            engine.sync().is_enabled(deck).to_string(),
        ));
    }
    for ch in 0..mixer.num_channels() {
        let c = format!("mixer.ch{ch}");
        rows.push((format!("{c}.trim"), format!("{:.3}", mixer.trim(ch))));
        rows.push((format!("{c}.fader"), format!("{:.3}", mixer.fader(ch))));
        rows.push((format!("{c}.filter"), format!("{:.3}", mixer.filter(ch))));
        for band in 0..3 {
            rows.push((
                format!("{c}.eq.{band}"),
                format!("{:.2}", mixer.eq(ch, band)),
            ));
        }
        rows.push((format!("{c}.pfl"), mixer.pfl(ch).to_string()));
        rows.push((
            format!("{c}.xfader"),
            format!("{:?}", mixer.crossfader_assign(ch)),
        ));
    }
    rows.push((
        "mixer.master".to_owned(),
        format!("{:.3}", mixer.master_gain()),
    ));
    rows.push((
        "mixer.crossfader".to_owned(),
        format!("{:.3}", mixer.crossfader()),
    ));
    rows.push((
        "mixer.crossfader.curve".to_owned(),
        format!("{:?}", mixer.crossfader_curve()),
    ));
    rows.push((
        "mixer.cue.mix".to_owned(),
        format!("{:.3}", mixer.cue_mix()),
    ));
    rows.push((
        "mixer.cue.level".to_owned(),
        format!("{:.3}", mixer.cue_level()),
    ));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameter_rows_use_the_router_ids_and_read_the_engine() {
        let (engine, _audio) = DjEngine::new(2);
        engine.mixer().set_master_gain(0.25);
        let rows = parameter_rows(&engine, 3);
        let get = |id: &str| rows.iter().find(|(k, _)| k == id).map(|(_, v)| v.clone());
        assert_eq!(get("router.unknown").as_deref(), Some("3"));
        assert_eq!(get("mixer.master").as_deref(), Some("0.250"));
        assert_eq!(get("deck.1.rate").as_deref(), Some("1.0000"));
        assert!(get("mixer.ch0.fader").is_some());
    }

    #[test]
    fn device_rows_show_the_latency_figures() {
        let rows = device_rows(&DeviceStatus {
            name: "mock".into(),
            sample_rate: 48_000.0,
            buffer_size: 256,
            round_trip_ms: 5.33,
            callback_avg_us: 120.0,
            callback_max_us: 300.0,
            budget_us: 5333.0,
            cpu_load: 0.025,
            xruns: 2,
            callbacks: 1000,
        });
        assert!(rows.iter().any(|(k, v)| k == "cpu load" && v == "2.5 %"));
        assert!(rows.iter().any(|(k, v)| k == "xruns" && v == "2"));
    }
}
