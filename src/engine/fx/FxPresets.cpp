#include "FxPresets.h"

namespace openmixer::fx
{

const std::vector<FxPreset>& builtInPresets()
{
    static const std::vector<FxPreset> presets {
        { "Dotted Throw", "echo", 0.5F, {{ { "time", 0.75F }, { "feedback", 0.55F }, { "tone", 0.45F } }} },
        { "Big Room", "reverb", 0.35F, {{ { "size", 0.85F }, { "damp", 0.3F }, { "width", 1.0F } }} },
        { "Slow Jet", "flanger", 0.8F, {{ { "time", 16.0F }, { "depth", 0.9F }, { "feedback", 0.6F } }} },
        { "Bar Swirl", "phaser", 0.7F, {{ { "time", 4.0F }, { "depth", 0.7F }, { "feedback", 0.5F } }} },
        { "Lo-Fi Grit", "crush", 0.6F, {{ { "bits", 6.0F }, { "downsample", 6.0F } }} },
        { "Sixteenth Chop", "gate", 1.0F, {{ { "time", 0.25F }, { "duty", 0.5F } }} },
        { "Half Roll", "roll", 1.0F, {{ { "time", 0.5F } }} },
        { "Power Down", "tapestop", 1.0F, {{ { "time", 2.0F } }} },
    };
    return presets;
}

const FxPreset* findPreset (std::string_view name) noexcept
{
    for (const auto& p : builtInPresets())
        if (p.name == name)
            return &p;
    return nullptr;
}

bool applyPreset (FxUnit& unit, const FxPreset& preset)
{
    auto& effect = unit.getEffect();
    if (effect.getName() != preset.effect)
        return false;
    unit.setMix (preset.mix);
    for (const auto& [name, value] : preset.params)
        if (! name.empty())
            effect.setParam (effect.findParam (name), value);
    return true;
}

} // namespace openmixer::fx
