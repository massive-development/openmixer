# Mapping editor (Todo 36, logic and view level)

> The `om-ui-gpui` code named below was removed on 2026-11-21 (last commit with it: `117ea60`; `git show 117ea60:<path>`). The `om-ui` models and `openmixer` services stay; the Tauri window gets the views as they are ported.

- `om-midi/src/editor.rs` (`MapEditor`): the edit model over `MidiMapper`. Add, delete, duplicate, undo and redo, conflicts and resolution, learn (`learn_into`, `learn_new`, `poll_learn`; the page's `learn_selected` calls `learn_into`), save with a `.bak`, import, export, reset to the default map, and the unsaved guard. Edits reach the running mapper at once.
- `om-ui/src/mapping_editor.rs` (`MappingEditorPage`): the view-model over `MapEditor`: filter, rows, the binding form, the monitor lines, status text. `tick()` completes a learn; it is called from the UI pump.
- `om-ui-gpui/src/settings_view/mapping.rs`: the Mapping settings tab. `SettingsState::mapping_tick` is called every frame by `Shell::tick` (`om-ui-gpui/src/app.rs`), not only while the overlay is drawn, so a learn started and then closed is still recorded.
- `openmixer/src/composition.rs`: `AppComposition::mapping_editor()` builds the page over the attached MIDI service (`None` until `attach_midi`). `--settings=mapping` opens the tab.
- Demo: `--demo-map` merges the fixture maps into `<scratch>/map/demo-map.json` on the mock controller.
