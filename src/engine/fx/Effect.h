// Base for one FX algorithm: declares its parameters, smooths them and renders a wet signal.
// Threads: setParam on the control thread; prepare on the control thread before insertion;
// reset/process on the audio thread (no allocation, locks or IO).
#pragma once

#include "FxMath.h"

#include <juce_core/juce_core.h>

#include <array>
#include <atomic>
#include <initializer_list>
#include <string_view>

namespace openmixer::fx
{

// Tempo range the FX buffers are sized for; beat-synced times are clamped to it.
inline constexpr double kMinBpm = 50.0;
inline constexpr double kMaxBpm = 250.0;

struct FxContext
{
    double sampleRate = 44100.0;
    double samplesPerBeat = 22050.0;
};

struct FxParamSpec
{
    std::string_view name;
    float minValue = 0.0F, maxValue = 1.0F, defaultValue = 0.0F;
    double smoothSeconds = 0.02;   // 0 = jump instantly (step-like controls)
};

class Effect
{
public:
    static constexpr int kMaxParams = 4;

    explicit Effect (std::initializer_list<FxParamSpec> specs) noexcept;
    virtual ~Effect() = default;

    virtual std::string_view getName() const noexcept = 0;
    // Tail effects (echo, reverb) keep sounding after disable until they fall silent.
    virtual bool hasTail() const noexcept { return false; }
    // Continuous wet silence (samples) after which a tail counts as finished.
    virtual int tailSilenceWindow (const FxContext&) const noexcept { return 0; }

    void prepare (double sampleRate, int maxBlockSize);
    virtual void reset() noexcept = 0;
    // Audio thread, when a unit goes idle: reset small state now and leave big buffers to
    // clearSome() so no callback pays for a large memset. Defaults to a full reset().
    virtual void beginReset() noexcept { reset(); }
    // Clears up to maxFrames of pending buffer; true when nothing is left (stale audio gone).
    virtual bool clearSome (int /*maxFrames*/) noexcept { return true; }
    // In place: left/right hold the effect input and receive the fully wet output.
    void process (float* left, float* right, int numSamples, const FxContext& ctx) noexcept;

    int getNumParams() const noexcept { return numParams; }
    const FxParamSpec& getParamSpec (int index) const noexcept;
    int findParam (std::string_view name) const noexcept;
    bool setParam (int index, float value) noexcept;
    float getParamTarget (int index) const noexcept;

protected:
    virtual void prepareEffect (double sampleRate, int maxBlockSize) = 0;
    virtual void render (float* left, float* right, int numSamples, const FxContext& ctx) noexcept = 0;

    // Advance all parameter smoothers by one sample; call once per rendered sample.
    void tickParams() noexcept;
    float param (int index) const noexcept { return smoothers[static_cast<size_t> (index)].current(); }

private:
    std::array<FxParamSpec, kMaxParams> specs {};
    std::array<std::atomic<float>, kMaxParams> targets {};
    std::array<float, kMaxParams> blockTargets {};
    std::array<Smoother, kMaxParams> smoothers {};
    int numParams = 0;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR (Effect)
};

} // namespace openmixer::fx
