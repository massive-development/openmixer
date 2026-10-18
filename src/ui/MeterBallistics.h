// Meter ballistics without any drawing: instant attack, exponential release, a peak-hold tick and a
// latching clip flag. Fed one linear peak per UI frame with the frame time, so it is frame-rate
// independent and testable with synthetic levels. Message thread only.
#pragma once

namespace openmixer::ui
{

struct MeterParams
{
    float floorDb = -48.0F, clipLevel = 0.99F;
    float releaseSeconds = 0.3F, holdSeconds = 1.0F, clipSeconds = 2.0F;

    static MeterParams fromTheme();   // reads the current theme tokens
};

class MeterBallistics
{
public:
    explicit MeterBallistics (MeterParams p = MeterParams::fromTheme()) : params (p) {}

    // Returns true when anything visible changed.
    bool update (float linearPeak, float dtSeconds);
    void resetClip() noexcept { clipRemaining = 0.0F; clipping = false; }

    float level() const noexcept { return levelProportion; }      // 0..1 bar height
    float peakHold() const noexcept { return peakProportion; }    // 0..1 hold tick height
    bool isClipping() const noexcept { return clipping; }

    const MeterParams& getParams() const noexcept { return params; }
    void setParams (const MeterParams& p) noexcept { params = p; }
    [[nodiscard]] static float proportionOf (float linear, float floorDb) noexcept;

private:
    void updateClip (float linearPeak, float dt) noexcept;

    MeterParams params;
    float levelProportion = 0.0F, peakProportion = 0.0F, holdRemaining = 0.0F, clipRemaining = 0.0F;
    bool clipping = false;
};

} // namespace openmixer::ui
