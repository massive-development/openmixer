// Original OpenMixer starting points for each built-in effect (data, not code paths).
#pragma once

#include "FxUnit.h"

#include <array>
#include <string_view>
#include <utility>
#include <vector>

namespace openmixer::fx
{

struct FxPreset
{
    std::string_view name, effect;
    float mix = 1.0F;
    std::array<std::pair<std::string_view, float>, Effect::kMaxParams> params {};
};

const std::vector<FxPreset>& builtInPresets();
const FxPreset* findPreset (std::string_view name) noexcept;
// Control thread. Returns false if the preset targets a different effect than the unit runs.
bool applyPreset (FxUnit& unit, const FxPreset& preset);

} // namespace openmixer::fx
