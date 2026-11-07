//! The plugin window call sequence against the fixture's GUI stub (ADR 0007, "GUI").
//!
//! These tests run without a display: the fixture accepts any X11 window id and answers every call.
//! They prove the host makes the CLAP calls in order and passes the right data. They do NOT prove that
//! a real plugin's window shows up inside a GPUI window; that needs a display and is unverified.
#![cfg(feature = "clap")]

mod clap_fixture;

use clap_fixture::{fixture, PLUGIN_ID};
use om_engine::StageChain;
use om_plugins::clap::{
    ClapLoader, ClapPlugin, GuiApi, GuiPlan, GuiSession, ParentWindow, PluginGui,
};
use om_plugins::{Plugin, PluginError, PluginHost, ProcessStage, StageFactory, StageKind};
use std::sync::Arc;

#[derive(Default)]
struct Capture {
    factory: Option<StageFactory>,
}

impl PluginHost for Capture {
    fn register_stage(
        &mut self,
        _: &str,
        _: StageKind,
        f: StageFactory,
    ) -> Result<(), PluginError> {
        self.factory = Some(f);
        Ok(())
    }
    fn register_midi_map(&mut self, _: &str, _: &str) -> Result<(), PluginError> {
        Ok(())
    }
    fn register_parameter_target(
        &mut self,
        _: &str,
        _: Arc<dyn om_core::ParameterTarget>,
    ) -> Result<(), PluginError> {
        Ok(())
    }
    fn register_theme(&mut self, _: &str) -> Result<(), PluginError> {
        Ok(())
    }
}

/// A live stage (kept so the instance stays alive) and its window handle.
struct Rig {
    _plugin: ClapPlugin,
    _stage: Box<dyn ProcessStage>,
    gui: PluginGui,
}

fn rig() -> Rig {
    let mut plugin = ClapLoader::load_typed(&fixture(None), PLUGIN_ID).expect("load");
    let mut capture = Capture::default();
    plugin.on_load(&mut capture);
    let stage = (capture.factory.expect("registered"))();
    let gui = plugin.stage_states()[0].gui().expect("window handle");
    Rig {
        _plugin: plugin,
        _stage: stage,
        gui,
    }
}

const PARENT: ParentWindow = ParentWindow::X11(0x0040_0007);

#[test]
fn x11_sessions_embed_and_wayland_sessions_get_the_parameter_panel() {
    let rig = rig();
    assert_eq!(
        rig.gui.plan(GuiSession::X11),
        GuiPlan::Embedded(GuiApi::X11)
    );
    // The fixture is X11-only: on Wayland there is no XWayland, so the generic panel is used.
    assert_eq!(rig.gui.plan(GuiSession::Wayland), GuiPlan::Parameters);
    assert_eq!(rig.gui.plan(GuiSession::Other), GuiPlan::Parameters);
}

#[test]
fn an_embedded_window_opens_resizes_and_closes() {
    let rig = rig();
    let size = rig
        .gui
        .open(GuiPlan::Embedded(GuiApi::X11), Some(PARENT), 1.0)
        .expect("opens");
    assert_eq!(size, Some((320, 200)), "the size the plugin reports");
    rig.gui.resize(400, 300).expect("resize");
    assert!(
        rig.gui.resize(9000, 9000).is_err(),
        "the plugin refuses a huge window"
    );
    assert_eq!(rig.gui.take_resize_request(), None);
    rig.gui.close();
    rig.gui.close(); // closing twice is harmless
    assert!(rig.gui.resize(400, 300).is_err(), "no window is open");
}

#[test]
fn a_floating_window_opens_without_a_parent_or_above_one() {
    let rig = rig();
    rig.gui
        .open(GuiPlan::Floating(GuiApi::X11), None, 1.0)
        .expect("floating, no parent");
    rig.gui
        .open(GuiPlan::Floating(GuiApi::X11), Some(PARENT), 2.0)
        .expect("reopened above a window");
    rig.gui.close();
}

#[test]
fn bad_requests_are_refused_without_leaving_a_window_open() {
    let rig = rig();
    let embedded = GuiPlan::Embedded(GuiApi::X11);
    assert!(
        rig.gui.open(embedded, None, 1.0).is_err(),
        "embedding needs a parent"
    );
    assert!(rig.gui.open(GuiPlan::Parameters, None, 1.0).is_err());
    // The Wayland API is not supported by the fixture: create fails and nothing stays open.
    assert!(rig
        .gui
        .open(GuiPlan::Floating(GuiApi::Wayland), None, 1.0)
        .is_err());
    assert!(rig.gui.resize(10, 10).is_err());
}

#[test]
fn dropping_the_stage_with_a_window_open_destroys_it_first() {
    let rig = rig();
    rig.gui
        .open(GuiPlan::Embedded(GuiApi::X11), Some(PARENT), 1.0)
        .expect("opens");
    let gui = rig.gui.clone();
    drop(rig);
    // The control thread is gone; calls report that instead of hanging or crashing.
    assert!(gui.open(GuiPlan::Floating(GuiApi::X11), None, 1.0).is_err());
    gui.close();
}

#[test]
fn a_stage_chain_still_processes_while_a_window_is_open() {
    let mut plugin = ClapLoader::load_typed(&fixture(None), PLUGIN_ID).expect("load");
    let mut capture = Capture::default();
    plugin.on_load(&mut capture);
    let (mut chain, insert) = StageChain::new();
    chain.prepare(48_000.0, 128);
    assert!(insert.set(0, Some((capture.factory.expect("registered"))())));
    let gui = plugin.stage_states()[0].gui().expect("window handle");
    gui.open(GuiPlan::Embedded(GuiApi::X11), Some(PARENT), 1.0)
        .expect("opens");
    let (mut l, mut r) = (vec![1.0_f32; 128], vec![1.0_f32; 128]);
    chain.process(&mut l, &mut r);
    assert!((l[0] - 0.5).abs() < 1e-6);
    gui.close();
    drop(chain);
    insert.reclaim();
}
