#include "FxRack.h"

namespace openmixer::fx
{

FxRack::FxRack (const std::vector<std::string>& layout, const FxFactory& factory)
{
    for (const auto& name : layout)
        if (auto effect = factory.create (name))
            units.push_back (std::make_unique<FxUnit> (std::move (effect)));
}

bool FxRack::setParameter (const juce::String& id, double value)
{
    if (id == "bpm")
    {
        setTempo (value);
        return true;
    }
    const auto dot = id.indexOfChar ('.');
    const auto digits = id.substring (2, dot);
    if (! id.startsWith ("fx") || dot < 0 || digits.isEmpty() || ! digits.containsOnly ("0123456789"))
        return false;
    auto* unit = getUnit (digits.getIntValue());
    return unit != nullptr && unit->setParameter (id.substring (dot + 1), value);
}

void FxRack::setTempo (double bpm) noexcept
{
    for (auto& unit : units)
        unit->setTempo (bpm);
}

void FxRack::setTempoFromGrid (const BeatGrid& grid, double playbackRate) noexcept
{
    setTempo (grid.getBpm() * playbackRate);
}

FxUnit* FxRack::getUnit (int index) noexcept
{
    return index >= 0 && index < getNumUnits() ? units[static_cast<size_t> (index)].get() : nullptr;
}

int FxRack::findUnit (std::string_view effectName) const noexcept
{
    for (size_t i = 0; i < units.size(); ++i)
        if (units[i]->getEffect().getName() == effectName)
            return static_cast<int> (i);
    return -1;
}

void FxRack::prepare (double sampleRate, int maxBlockSize)
{
    for (auto& unit : units)
        unit->prepare (sampleRate, maxBlockSize);
}

void FxRack::process (float* left, float* right, int numSamples) noexcept
{
    for (auto& unit : units)
        unit->process (left, right, numSamples);
}

} // namespace openmixer::fx
