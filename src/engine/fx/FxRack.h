// Series chain of FX units sharing one tempo; plugs into Mixer::setChannelInsert/setMasterInsert.
// Threads: build and setParameter/setTempo on the control thread; process on the audio thread.
#pragma once

#include "FxFactory.h"
#include "FxUnit.h"
#include "engine/BeatGrid.h"

#include <memory>
#include <string>
#include <vector>

namespace openmixer::fx
{

class FxRack final : public ProcessStage, public ParameterTarget
{
public:
    // One unit per known effect name, in order; unknown names are skipped.
    explicit FxRack (const std::vector<std::string>& layout = FxFactory::builtInNames(),
                     const FxFactory& factory = FxFactory::withBuiltIns());

    // Ids: "bpm", "fx<N>.enable", "fx<N>.mix", "fx<N>.<effect param>" (N = unit index).
    bool setParameter (const juce::String& id, double value) override;
    void setTempo (double bpm) noexcept;
    // Follow a deck's grid tempo; call from the control thread whenever the grid or rate changes.
    void setTempoFromGrid (const BeatGrid& grid, double playbackRate = 1.0) noexcept;

    int getNumUnits() const noexcept { return static_cast<int> (units.size()); }
    FxUnit* getUnit (int index) noexcept;
    // Index of the first unit running the named effect, or -1.
    int findUnit (std::string_view effectName) const noexcept;

    void prepare (double sampleRate, int maxBlockSize) override;
    void process (float* left, float* right, int numSamples) noexcept override;

private:
    std::vector<std::unique_ptr<FxUnit>> units;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR (FxRack)
};

} // namespace openmixer::fx
