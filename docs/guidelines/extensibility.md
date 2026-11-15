# Extensibility and user freedom

Users must always be able to modify OpenMixer with external plugins. Nothing may be locked down.

- **Extension points by default:** new features expose hooks for external plugins — audio pre/post-processing (per deck, channel and master), effects, analysis, library import/export, controller mappings, and UI/theming (colours, layouts, skins).
- **No hardcoded restrictions:** don't hardcode behaviour, limits, themes or signal paths that a plugin could reasonably replace or extend. Prefer registries, traits and data-driven configuration (serde-loaded files) over fixed code paths.
- **Stable plugin API:** keep extension traits small, documented and versioned; don't break them silently. Plugins must not need to modify core source.
- **No gatekeeping:** don't add signing, allow-lists or sandbox rules that stop users loading their own plugins (opt-in safety warnings are fine). Respect AGPLv3 obligations for plugins linked into the app.
- **Plugins load in-process, from a C-ABI shared library, or (feature `clap`) as CLAP plugins.** `om-plugins` has the plugin API, registry and loaders (see `openmixer-workbench/docs/spikes/DECISIONS.md` and ADR 0007). Keep new extension points as traits and registries in the owning crate, so they can be exposed later without restructuring. A plugin's own state (`Plugin::save_state` / `restore_state`, opaque bytes) is kept by the registry and stored by the app in `plugin-state.json` beside the preferences (`openmixer::plugin_state`, versioned; ADR 0007 step 6).
- **PR check:** every PR must state whether it adds, changes or could block an extension point.
