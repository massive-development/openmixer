#include "PreferencesApplier.h"

namespace openmixer::ui
{

int PreferencesApplier::apply (const Preferences& p)
{
    int pushed = 0;
    const bool first = ! applied.has_value();
    const Preferences& old = first ? p : *applied;

    const bool pluginsChanged = first || old.effectivePluginFolder() != p.effectivePluginFolder();
    if (pluginsChanged)
        targets.applyPluginFolder (p.effectivePluginFolder()), ++pushed;
    if (pluginsChanged || old.theme != p.theme)   // a plugin theme may have just appeared or gone
        targets.applyTheme (p.theme), ++pushed;
    if (first || ! (old.audio == p.audio))
        targets.applyAudio (p.audio), ++pushed;
    if (first || ! (old.midi == p.midi))
        targets.applyMidi (p.midi), ++pushed;
    applied = p;
    return pushed;
}

} // namespace openmixer::ui
