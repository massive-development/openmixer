//! Loading and listing CLAP plugins (ADR 0007, migration step 3). Builds the fixture plugin
//! (`examples/plugins/clap-gain-fixture`, made with clack-plugin) once per run and loads it.

#![cfg(feature = "clap")]

mod clap_fixture;

use clap_fixture::{fixture, other_library};
use om_plugins::clap::{ClapLoader, StageRole};
use om_plugins::{PluginError, PluginRegistry};

#[test]
fn scan_lists_the_plugin_without_creating_an_instance() {
    let infos = ClapLoader::scan(&fixture(None)).expect("scan");
    assert_eq!(infos.len(), 1);
    let info = &infos[0];
    assert_eq!(info.id, clap_fixture::PLUGIN_ID);
    assert_eq!(
        (info.name.as_str(), info.vendor.as_str()),
        ("OpenMixer Test Gain", "OpenMixer")
    );
    assert!(
        info.features.iter().any(|f| f == "audio-effect"),
        "{:?}",
        info.features
    );
}

#[test]
fn load_reads_the_ports_and_parameters() {
    let plugin = ClapLoader::load(&fixture(None), clap_fixture::PLUGIN_ID).expect("load");
    assert_eq!(plugin.info().id, clap_fixture::PLUGIN_ID);
    let params = plugin.parameters();
    assert_eq!(params.len(), 1);
    assert_eq!(
        (
            params[0].name.as_str(),
            params[0].min,
            params[0].max,
            params[0].default
        ),
        ("Gain", 0.0, 2.0, 0.5)
    );
    assert_eq!(plugin.parameter(&params[0].id), Some(0.5));
    assert_eq!(plugin.parameter("999"), None);
}

#[test]
fn an_unknown_plugin_id_is_refused() {
    let err = ClapLoader::load(&fixture(None), "no.such.plugin")
        .err()
        .expect("refused");
    assert!(matches!(err, PluginError::UnknownStage(_)), "{err}");
}

#[test]
fn files_that_are_not_clap_plugins_are_refused_with_a_typed_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let garbage = dir.path().join("garbage.clap");
    std::fs::write(&garbage, b"this is not a library").expect("write");
    for path in [garbage, other_library()] {
        let err = ClapLoader::scan(&path).expect_err("refused");
        assert!(
            matches!(err, PluginError::InvalidLibrary { .. }),
            "{}: {err}",
            path.display()
        );
    }
    let missing = dir.path().join("missing.clap");
    assert!(matches!(
        ClapLoader::scan(&missing),
        Err(PluginError::Io { .. })
    ));
}

#[test]
fn the_descriptor_is_a_stereo_effect() {
    let plugin = ClapLoader::load_typed(&fixture(None), clap_fixture::PLUGIN_ID).expect("load");
    let d = plugin.descriptor();
    assert_eq!(d.role, StageRole::Effect);
    assert_eq!((d.inputs.len(), d.outputs.len()), (1, 1));
    assert_eq!((d.inputs[0].channels, d.outputs[0].channels), (2, 2));
}

#[test]
fn the_registry_refuses_clap_files_until_the_warning_is_accepted() {
    let path = fixture(None);
    let mut registry = PluginRegistry::new();
    let mut errors = Vec::new();
    let err = registry.load_clap(&path, &mut errors).expect_err("refused");
    assert!(matches!(err, PluginError::NotAcknowledged(_)), "{err}");
    assert!(registry.plugin_ids().is_empty());

    registry.acknowledge_clap_warning();
    assert_eq!(
        registry.load_clap(&path, &mut errors).expect("loads"),
        1,
        "{errors:?}"
    );
    assert_eq!(registry.plugin_ids(), [clap_fixture::PLUGIN_ID]);
    // A second load of the same file is a duplicate id, reported and not fatal.
    assert_eq!(registry.load_clap(&path, &mut errors).expect("second"), 0);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(registry.unload(clap_fixture::PLUGIN_ID));
}
