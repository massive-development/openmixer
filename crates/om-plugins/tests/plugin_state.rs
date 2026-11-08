//! Plugin state through the registry, for in-process plugins (no `clap` feature needed): handed to a
//! plugin when it loads, kept when it unloads, refusals collected.

use om_plugins::{Plugin, PluginError, PluginHost, PluginInfo, PluginRegistry};
use std::sync::Mutex;

/// A plugin that keeps one byte of state and refuses a state of any other length.
struct Counter(Mutex<u8>);

impl Plugin for Counter {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: "test.counter".into(),
            name: "Counter".into(),
            version: "1".into(),
        }
    }

    fn on_load(&mut self, _host: &mut dyn PluginHost) {}

    fn save_state(&self) -> Result<Option<Vec<u8>>, PluginError> {
        Ok(Some(vec![*self.0.lock().unwrap()]))
    }

    fn restore_state(&self, state: &[u8]) -> Result<(), PluginError> {
        let [byte] = state else {
            return Err(PluginError::State {
                plugin: "test.counter".into(),
                reason: "wrong length".into(),
            });
        };
        *self.0.lock().unwrap() = *byte;
        Ok(())
    }
}

#[test]
fn state_is_handed_over_at_load_kept_at_unload_and_refusals_are_collected() {
    let mut reg = PluginRegistry::new();
    reg.set_saved_states([("test.counter".to_owned(), vec![9])].into());
    reg.add_plugin(Box::new(Counter(Mutex::new(0)))).unwrap();
    assert_eq!(
        reg.plugin_states()["test.counter"],
        vec![9],
        "restored at load"
    );
    assert!(reg.take_state_errors().is_empty());

    reg.restore_plugin_state("test.counter", &[3]).unwrap();
    assert!(reg.restore_plugin_state("test.counter", &[1, 2]).is_err());
    assert!(reg.restore_plugin_state("absent", &[1]).is_err());
    assert!(reg.unload("test.counter"));
    assert_eq!(
        reg.plugin_states()["test.counter"],
        vec![3],
        "the state outlives the unload"
    );

    reg.set_saved_states([("test.counter".to_owned(), vec![1, 2, 3])].into());
    reg.add_plugin(Box::new(Counter(Mutex::new(0)))).unwrap();
    assert_eq!(
        reg.take_state_errors().len(),
        1,
        "a refused state is reported"
    );
    assert!(reg.take_state_errors().is_empty(), "and reported once");
}
