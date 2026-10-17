// Tape stop: playback speed (and pitch) ramps from 1 to 0 over a beat-synced time.
#pragma once

#include "Effect.h"

#include <array>
#include <cstdint>
#include <vector>

namespace openmixer::fx
{

class TapeStopFx final : public Effect
{
public:
    enum Param { kTime };
    static constexpr float kMaxBeats = 8.0F;

    TapeStopFx() noexcept;
    std::string_view getName() const noexcept override { return "tapestop"; }
    // Engaging restarts the slow-down from full speed.
    void reset() noexcept override;

protected:
    void prepareEffect (double sampleRate, int maxBlockSize) override;
    void render (float* left, float* right, int numSamples, const FxContext& ctx) noexcept override;

private:
    float readAt (int channel, double position) const noexcept;

    std::array<std::vector<float>, 2> history;
    std::int64_t written = 0;
    double readPos = 0.0, speed = 1.0;
};

} // namespace openmixer::fx
