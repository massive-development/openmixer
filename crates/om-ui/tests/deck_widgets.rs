//! Routing, engine mirroring and the deck logic, headless.
//! Cases that need a decoded track (the audio pump) are not covered here.

mod common;

use common::Rig;
use om_core::DeckSnapshot;
use om_ui::binding::DeckBinding;
use om_ui::waveform::{StripPlan, View};
use om_ui::widgets::deck::{grid_lines_for, DeckWidget, TrackHeader, TrackInfo, WaveformMeta};
use om_ui::widgets::hotcues::{HotcuePads, Modifiers};
use om_ui::widgets::loops::{loop_param, LoopControls, LoopFrame, SIZES};
use om_ui::widgets::pitch::PitchFader;
use om_ui::widgets::transport::{TransportButton, TransportPanel};

fn binding(rig: &Rig, deck: usize) -> DeckBinding {
    DeckBinding::new(rig.engine.clone(), rig.recorder.clone(), deck)
}

#[test]
fn transport_buttons_route_their_ids_and_mirror_engine_state() {
    let rig = Rig::new(2);
    rig.engine.set_grid(0, Some(Rig::grid(120.0)));
    let panel = TransportPanel::new(binding(&rig, 0));

    panel.press(TransportButton::Play);
    assert!(rig.recorder.saw("deck.0.play", 1.0));
    assert!(rig.engine.deck(0).is_playing());
    assert_eq!(panel.play_label(), "Pause");
    panel.press(TransportButton::Play);
    assert!(rig.recorder.saw("deck.0.play", 0.0));

    panel.press(TransportButton::Sync);
    assert!(rig.recorder.saw("deck.0.sync", 1.0));
    assert!(rig.engine.sync().is_enabled(0));
    assert!(panel.toggle_states()[2], "sync reads on");

    panel.press(TransportButton::Master);
    assert!(rig.recorder.saw("deck.0.sync.master", 1.0));
    assert_eq!(rig.engine.sync().leader(), Some(0));
    panel.press(TransportButton::Master);
    assert!(rig.recorder.saw("deck.0.sync.master", 0.0));
    assert_eq!(rig.engine.sync().leader(), None);

    panel.press(TransportButton::KeyLock);
    assert!(rig.recorder.saw("deck.0.keylock", 1.0));
    assert!(rig.engine.deck(0).is_keylock());
    assert!(
        rig.engine.deck_control(0).is_quantize(),
        "quantize starts on"
    );
    panel.press(TransportButton::Quantize);
    assert!(rig.recorder.saw("deck.0.quantize", 0.0));
    assert!(!rig.engine.deck_control(0).is_quantize());

    panel.press(TransportButton::Slip);
    assert!(rig.recorder.saw("deck.0.slip", 1.0));
    assert!(rig.engine.deck_control(0).is_slipping());
}

#[test]
fn cue_stops_and_returns_to_start() {
    let rig = Rig::new(2);
    rig.engine.deck(0).play();
    let panel = TransportPanel::new(binding(&rig, 0));
    panel.press(TransportButton::Cue);
    assert!(rig.recorder.saw("deck.0.play", 0.0));
    assert!(rig.recorder.saw("deck.0.seek", 0.0));
    assert!(!rig.engine.deck(0).is_playing());
}

#[test]
fn transport_of_second_deck_uses_its_own_prefix() {
    let rig = Rig::new(2);
    let panel = TransportPanel::new(binding(&rig, 1));
    panel.press(TransportButton::KeyLock);
    assert!(rig.recorder.saw("deck.1.keylock", 1.0));
    assert!(rig.engine.deck(1).is_keylock());
    assert!(!rig.engine.deck(0).is_keylock());
}

#[test]
fn pitch_fader_routes_rate_and_range_limits_it() {
    let rig = Rig::new(2);
    let mut pitch = PitchFader::new(binding(&rig, 0));
    assert!(
        rig.recorder.is_empty(),
        "building the widget must not touch the engine"
    );

    let v = pitch.user_set(1.05);
    assert!((v - 1.05).abs() < 1e-9);
    assert!(rig.recorder.saw("deck.0.rate", 1.05));
    assert!((rig.engine.deck(0).rate() - 1.05).abs() < 1e-9);

    let v = pitch.user_set(1.5);
    assert!((v - 1.08).abs() < 1e-9, "+-8% range clamps");

    assert_eq!(pitch.set_range_index(2), None);
    assert!((pitch.range() - 0.5).abs() < 1e-12);
    pitch.user_set(1.4);
    assert!((rig.engine.deck(0).rate() - 1.4).abs() < 1e-9);

    let pulled = pitch.set_range_index(0);
    assert!(
        pulled.is_some_and(|r| (r - 1.08).abs() < 1e-9),
        "narrowing the range pulls the tempo back"
    );
    assert!((rig.engine.deck(0).rate() - 1.08).abs() < 1e-9);
}

#[test]
fn pitch_fader_follows_rate_changes_from_elsewhere() {
    let rig = Rig::new(2);
    let mut pitch = PitchFader::new(binding(&rig, 0));
    rig.engine.deck(0).set_rate(1.03);
    rig.recorder.clear();
    pitch.refresh(false);
    assert!((pitch.fader_value() - 1.03).abs() < 1e-9);
    assert!(
        rig.recorder.is_empty(),
        "following the engine must not echo back"
    );
}

#[test]
fn hotcue_pads_set_jump_and_clear() {
    let rig = Rig::new(2);
    let pads = HotcuePads::new(binding(&rig, 0));
    assert!(rig.engine.deck(0).hotcue(2).is_none());
    assert!(pads.click(2, Modifiers::default()));
    assert!(rig.recorder.saw("deck.0.hotcue2", 1.0));
    assert!(rig.engine.deck(0).hotcue(2).is_some());
    let lit = pads.lit();
    assert!(lit[2], "set cue lights its pad");
    assert!(!lit[3]);

    assert!(pads.press_pad(2, true));
    assert!(rig.recorder.saw("deck.0.hotcue2.clear", 1.0));
    assert!(rig.engine.deck(0).hotcue(2).is_none());
    assert!(!pads.lit()[2]);

    rig.recorder.clear();
    assert!(!pads.press_pad(8, false));
    assert!(rig.recorder.is_empty(), "out-of-range pad sends nothing");
}

#[test]
fn modifier_click_clears_a_pad() {
    let rig = Rig::new(2);
    let pads = HotcuePads::new(binding(&rig, 0));
    pads.click(0, Modifiers::default());
    pads.click(
        0,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert!(rig.recorder.saw("deck.0.hotcue0.clear", 1.0));
    assert!(rig.engine.deck(0).hotcue(0).is_none());
}

#[test]
fn loop_buttons_route_exact_ids() {
    let rig = Rig::new(2);
    let loops = LoopControls::new(binding(&rig, 0));
    for (i, &beats) in SIZES.iter().enumerate() {
        rig.recorder.clear();
        loops.press_size(i);
        assert_eq!(rig.recorder.calls().len(), 1);
        assert_eq!(
            rig.recorder.calls()[0].0,
            format!("deck.0.{}", loop_param(beats))
        );
        assert_eq!(rig.recorder.calls()[0].1, 1.0);
    }
    assert_eq!(loop_param(0.25).to_string(), "loop.quantized.0.25");

    rig.recorder.clear();
    loops.exit();
    loops.reloop();
    assert!(rig.recorder.saw("deck.0.loop.exit", 1.0));
    assert!(rig.recorder.saw("deck.0.loop.reloop", 1.0));

    rig.recorder.clear();
    assert_eq!(loops.halve(), None);
    assert!(rig.recorder.is_empty(), "no loop to halve");
}

/// One frame's lit buttons and length come from one snapshot: a lit size button matches the length
/// reported beside it, and reloop is lit exactly when the loop is.
#[test]
fn loop_frame_lights_and_length_come_from_one_snapshot() {
    let grid = om_engine::grid::FixedTempoGrid::new(120.0, 44_100.0, 0);
    // 120 BPM: two beats per second, so 22,050 frames is one beat.
    let looping = DeckSnapshot {
        looping: true,
        loop_start: 0,
        loop_end: 22_050,
        ..DeckSnapshot::default()
    };
    let frame = LoopFrame::from_state(&looping, Some(&grid));
    assert!((frame.length_beats - 1.0).abs() < 1e-9);
    assert!(frame.reloop_lit);
    for (i, lit) in frame.size_lit.iter().enumerate() {
        assert_eq!(*lit, SIZES[i] == 1.0, "size {i}");
    }

    let exited = DeckSnapshot {
        looping: false,
        ..looping
    };
    let frame = LoopFrame::from_state(&exited, Some(&grid));
    assert!(!frame.reloop_lit);
    assert_eq!(
        frame.size_lit, [false; 8],
        "no lit size without an active loop"
    );

    let no_grid = LoopFrame::from_state(&looping, None);
    assert_eq!(no_grid.length_beats, 0.0);
    assert_eq!(no_grid.size_lit, [false; 8]);
}

#[test]
fn overview_click_maps_x_to_seek_fraction() {
    let rig = Rig::new(2);
    let overview = om_ui::widgets::deck::OverviewWaveform::new(binding(&rig, 0));
    assert_eq!(overview.seek_at(100.0, 400.0), 0.25);
    assert!(rig.recorder.saw("deck.0.seek", 0.25));
    assert_eq!(overview.seek_at(-30.0, 400.0), 0.0);
    assert_eq!(overview.seek_at(999.0, 400.0), 1.0);
    rig.recorder.clear();
    assert_eq!(overview.seek_at(300.0, 400.0), 0.75);
    assert!(
        rig.recorder.saw("deck.0.seek", 0.75),
        "a click reaches the same mapping"
    );
}

#[test]
fn zoom_repaints_only_when_something_changed() {
    let rig = Rig::new(2);
    let mut deck = DeckWidget::new(binding(&rig, 0));
    deck.zoom.refresh();
    let base = deck.zoom.repaint_requests;
    for _ in 0..5 {
        assert!(!deck.zoom.refresh());
    }
    assert_eq!(
        deck.zoom.repaint_requests, base,
        "an idle deck costs no repaints"
    );
    rig.engine.deck(0).set_hotcue(1, 5000);
    assert!(deck.zoom.refresh());
    assert_eq!(deck.zoom.repaint_requests, base + 1, "a new cue repaints");
    deck.zoom.refresh();
    assert_eq!(deck.zoom.repaint_requests, base + 1);
}

#[test]
fn zoom_scrolling_reuses_the_cached_strip_and_rerenders_only_when_needed() {
    let rig = Rig::new(2);
    let mut deck = DeckWidget::new(binding(&rig, 0));
    deck.zoom.set_waveform(Some(WaveformMeta {
        sample_rate: 44_100.0,
    }));
    assert!(deck.zoom.paint_strip(400, 120).is_some());
    assert_eq!(deck.zoom.cache_renders, 1);
    deck.zoom.paint_strip(400, 120);
    assert_eq!(deck.zoom.cache_renders, 1, "same view reuses the strip");

    deck.zoom.set_zoom_step(2);
    deck.zoom.paint_strip(400, 120);
    assert_eq!(deck.zoom.cache_renders, 2, "zoom change re-renders");
    deck.zoom.paint_strip(300, 120);
    assert_eq!(deck.zoom.cache_renders, 3, "resize re-renders");
    deck.zoom.set_waveform(Some(WaveformMeta {
        sample_rate: 44_100.0,
    }));
    deck.zoom.paint_strip(300, 120);
    assert_eq!(deck.zoom.cache_renders, 4, "new waveform re-renders");
}

#[test]
fn zoom_without_a_waveform_paints_nothing() {
    let rig = Rig::new(2);
    let mut deck = DeckWidget::new(binding(&rig, 0));
    assert_eq!(deck.zoom.paint_strip(400, 120), None);
    assert_eq!(deck.zoom.cache_renders, 0);
}

#[test]
fn zoom_grid_lines_appear_with_a_grid_and_zoom_stays_in_range() {
    let rig = Rig::new(2);
    let deck = DeckWidget::new(binding(&rig, 0));
    let view = View {
        centre_frame: 0,
        frames_per_pixel: 200.0,
        width: 400.0,
    };
    assert!(deck.zoom.grid_lines(&view).is_empty(), "no grid, no lines");
    rig.engine.set_grid(0, Some(Rig::grid(120.0)));
    assert!(!deck.zoom.grid_lines(&view).is_empty(), "grid lines appear");
}

#[test]
fn zoom_is_four_discrete_steps_in_seconds_and_the_wheel_stays_in_range() {
    let rig = Rig::new(2);
    let mut deck = DeckWidget::new(binding(&rig, 0));
    assert_eq!(deck.zoom.zoom_seconds(), 8.0, "default");
    deck.zoom.set_zoom_step(99);
    assert_eq!(deck.zoom.zoom_seconds(), 32.0, "clamped");
    deck.zoom.wheel(-1.0);
    assert_eq!(deck.zoom.zoom_seconds(), 32.0, "already widest");
    assert_eq!(deck.zoom.wheel(1.0), 2);
    assert_eq!(deck.zoom.zoom_seconds(), 16.0, "scroll down zooms in");
    for _ in 0..10 {
        deck.zoom.wheel(1.0);
    }
    assert_eq!(deck.zoom.zoom_seconds(), 4.0, "already closest");

    // The same step spans the same seconds whatever the strip width.
    deck.zoom.set_waveform(Some(WaveformMeta {
        sample_rate: 44_100.0,
    }));
    for width in [800.0, 1900.0] {
        let view = deck.zoom.view(width);
        let seconds = view.frames_per_pixel * width / 44_100.0;
        assert!((seconds - 4.0).abs() < 1e-9, "{width}: {seconds}");
    }
}

#[test]
fn bars_only_when_beats_are_too_dense_to_draw() {
    // 120 BPM at 44.1 kHz is 22050 frames per beat. At 4410 frames per pixel a beat is 5 px apart,
    // closer than the 8 px minimum, so only bars (every four beats) remain.
    let grid = Rig::grid(120.0);
    let dense = View {
        centre_frame: 0,
        frames_per_pixel: 4_410.0,
        width: 400.0,
    };
    let lines = grid_lines_for(grid.as_ref(), &dense);
    assert!(!lines.is_empty());
    assert!(
        lines.iter().all(|l| l.bar),
        "only bars when beats are too dense"
    );
    let sparse = View {
        centre_frame: 0,
        frames_per_pixel: 1_000.0,
        width: 400.0,
    };
    assert!(
        grid_lines_for(grid.as_ref(), &sparse)
            .iter()
            .any(|l| !l.bar),
        "beats are drawn when there is room"
    );
}

#[test]
fn bars_carry_numbers_that_thin_out_before_they_touch() {
    let grid = Rig::grid(120.0); // a bar is 88_200 frames
                                 // 300 frames per pixel: a bar is 294 px wide, every bar is labelled, counting from 1.
    let roomy = View {
        centre_frame: 88_200 * 3,
        frames_per_pixel: 300.0,
        width: 1200.0,
    };
    let numbers: Vec<_> = grid_lines_for(grid.as_ref(), &roomy)
        .iter()
        .filter_map(|l| l.bar_number)
        .collect();
    assert!(numbers.len() >= 3, "{numbers:?}");
    assert!(numbers.windows(2).all(|w| w[1] == w[0] + 1));
    assert!(grid_lines_for(grid.as_ref(), &roomy)
        .iter()
        .filter(|l| !l.bar)
        .all(|l| l.bar_number.is_none()));
    // 2000 frames per pixel: a bar is 44 px, so only every second bar keeps its number.
    let tight = View {
        frames_per_pixel: 2_000.0,
        width: 1200.0,
        centre_frame: 88_200 * 20,
    };
    let numbers: Vec<_> = grid_lines_for(grid.as_ref(), &tight)
        .iter()
        .filter_map(|l| l.bar_number)
        .collect();
    assert!(numbers.windows(2).all(|w| w[1] == w[0] + 2), "{numbers:?}");
}

#[test]
fn hot_cues_reach_the_overview_markers_and_no_loop_means_no_region() {
    let rig = Rig::new(2);
    let deck = DeckWidget::new(binding(&rig, 0));
    rig.engine.set_grid(0, Some(Rig::grid(120.0)));
    let view = deck.zoom.view(800.0);
    assert!(deck.zoom.loop_region(&view).is_none());
    assert!(deck.overview.markers(400.0).cues.is_empty());
    rig.engine.deck(0).set_hotcue(1, 5000);
    let markers = deck.overview.markers(400.0);
    assert_eq!(markers.cues.len(), 1);
    assert_eq!(markers.cues[0].1, om_ui::waveform::cue_colour(1));
}

#[test]
fn header_shows_track_info_and_effective_bpm() {
    let rig = Rig::new(2);
    let mut header = TrackHeader::new(binding(&rig, 0));
    header.set_track(TrackInfo {
        title: "Night Drive".into(),
        artist: "Some Artist".into(),
        key: "Am".into(),
        camelot: "8A".into(),
        bpm: 124.0,
    });
    let t = header.text();
    assert_eq!(t.title, "Night Drive");
    assert_eq!(t.artist, "Some Artist");
    assert_eq!(t.key, "Am  8A");
    assert_eq!(t.bpm, "124.0");
    assert_eq!(t.elapsed, "0:00.0");

    rig.engine.deck(0).set_rate(1.05);
    assert_eq!(
        header.text().bpm,
        "130.2",
        "metadata bpm scaled by the pitch"
    );
    rig.engine.set_grid(0, Some(Rig::grid(100.0)));
    assert_eq!(
        header.text().bpm,
        "105.0",
        "the beat grid wins over metadata"
    );

    let empty = TrackHeader::new(binding(&rig, 1));
    assert_eq!(empty.text().title, "No track loaded");
}

#[test]
fn set_grid_reaches_the_engine() {
    let rig = Rig::new(2);
    let deck = DeckWidget::new(binding(&rig, 0));
    deck.set_grid(Some(Rig::grid(120.0)));
    assert!(rig.engine.deck_control(0).grid().is_some());
    deck.set_grid(None);
    assert!(rig.engine.deck_control(0).grid().is_none());
}

#[test]
fn zoom_view_centres_on_the_playhead_and_strip_plan_reuses_within_margin() {
    let rig = Rig::new(2);
    let mut deck = DeckWidget::new(binding(&rig, 0));
    deck.zoom.set_zoom_step(0);
    deck.zoom.set_waveform(Some(WaveformMeta {
        sample_rate: 44_100.0,
    }));
    let plan = deck.zoom.paint_strip(400, 120);
    assert!(matches!(plan, Some(StripPlan::Render)));
    assert_eq!(deck.zoom.view(400.0).x_of_frame(0), 200.0);
}
