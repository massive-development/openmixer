// Beat roll: captures one beat-division of audio when engaged and repeats it.
#pragma once

#include "Effect.h"

#include <array>
#include <vector>

namespace openmixer::fx
{

class RollFx final : public Effect
{
public:
    enum Param { kTime };
    static constexpr float kMaxBeats = 4.0F;

    RollFx() noexcept;
    std::string_view getName() const noexcept override { return "roll"; }
    // Engaging restarts the capture. reset() only rewinds counters (reads never pass `recorded`),
    // so it is O(1) on the audio thread and needs no sliced clear.
    void reset() noexcept override;

protected:
    void prepareEffect (double sampleRate, int maxBlockSize) override;
    void render (float* left, float* right, int numSamples, const FxContext& ctx) noexcept override;

private:
    static float edgeGain (int pos, int length, bool fadeIn) noexcept;

    std::array<std::vector<float>, 2> loop;
    int capacity = 0, recorded = 0, playPos = 0;
};

} // namespace openmixer::fx
