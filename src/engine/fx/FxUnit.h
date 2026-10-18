// One FX slot: wraps an Effect with enable, wet/dry mix, tempo and tail handling.
// Usable alone as a ProcessStage (channel/master insert) or inside an FxRack.
// Threads: setParameter/setTempo on the control thread; prepare before insertion; process
// on the audio thread (fixed-size scratch, no allocation, locks or IO).
#pragma once

#include "Effect.h"
#include "core/ParameterTarget.h"
#include "engine/ProcessStage.h"

#include <array>
#include <atomic>
#include <memory>

namespace openmixer::fx
{

class FxUnit final : public ProcessStage, public ParameterTarget
{
public:
    static constexpr int kChunk = 64;
    static constexpr int kClearSlice = 512;   // frames wiped per idle chunk after a tail ends

    explicit FxUnit (std::unique_ptr<Effect> effect);

    // Ids: "enable" (>= 0.5 on), "mix" (0..1), plus the effect's own names (e.g. "time").
    bool setParameter (const juce::String& id, double value) override;
    void setEnabled (bool on) noexcept { enabled.store (on, std::memory_order_relaxed); }
    void setMix (float mix) noexcept;
    void setTempo (double bpm) noexcept;

    bool isEnabled() const noexcept { return enabled.load (std::memory_order_relaxed); }
    // False once a disabled unit has finished its tail and stopped processing.
    bool isActive() const noexcept { return active.load (std::memory_order_relaxed); }
    float getMix() const noexcept { return mixTarget.load (std::memory_order_relaxed); }
    double getTempo() const noexcept { return bpm.load (std::memory_order_relaxed); }
    Effect& getEffect() noexcept { return *effect; }
    const Effect& getEffect() const noexcept { return *effect; }

    void prepare (double sampleRate, int maxBlockSize) override;
    void process (float* left, float* right, int numSamples) noexcept override;

private:
    void processChunk (float* left, float* right, int n) noexcept;
    void computeGains (int n, bool on) noexcept;
    void mixInto (float* left, float* right, int n) const noexcept;
    void updateIdle (int n, bool on) noexcept;
    void sliceClear (int frames) noexcept;
    float wetPeak (int n) const noexcept;

    std::unique_ptr<Effect> effect;
    std::atomic<bool> enabled { false }, active { false };
    std::atomic<float> mixTarget { 1.0F };
    std::atomic<double> bpm { 120.0 };

    FxContext ctx;
    Smoother enableRamp, mixRamp, beatSamples;
    std::array<float, kChunk> wetL {}, wetR {}, onGain {}, mixGain {};
    int quietSamples = 0;
    bool idle = true, clearing = false;   // clearing: stale buffer is still being wiped in slices

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR (FxUnit)
};

} // namespace openmixer::fx
