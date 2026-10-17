#include "FxUnit.h"

#include <juce_audio_basics/juce_audio_basics.h>

#include <algorithm>
#include <cmath>

namespace openmixer::fx
{

namespace
{
    constexpr auto kRelaxed = std::memory_order_relaxed;
    constexpr double kEnableRampSec = 0.01, kMixRampSec = 0.02, kTempoRampSec = 0.05;
    constexpr float kSilence = 1.0e-5F;   // -100 dBFS: a tail below this is finished

    double samplesPerBeat (double sampleRate, double bpm) noexcept
    {
        return sampleRate * 60.0 / bpm;
    }
}

FxUnit::FxUnit (std::unique_ptr<Effect> fx) : effect (std::move (fx))
{
    jassert (effect != nullptr);
}

bool FxUnit::setParameter (const juce::String& id, double value)
{
    if (id == "enable") { setEnabled (value >= 0.5); return true; }
    if (id == "mix")    { setMix (static_cast<float> (value)); return true; }
    if (id == "bpm")    { setTempo (value); return true; }
    return effect->setParam (effect->findParam (id.toStdString()), static_cast<float> (value));
}

void FxUnit::setMix (float mix) noexcept
{
    if (std::isfinite (mix))
        mixTarget.store (std::clamp (mix, 0.0F, 1.0F), kRelaxed);
}

void FxUnit::setTempo (double newBpm) noexcept
{
    if (std::isfinite (newBpm))
        bpm.store (std::clamp (newBpm, kMinBpm, kMaxBpm), kRelaxed);
}

void FxUnit::prepare (double sampleRate, int)
{
    const bool on = isEnabled();
    ctx.sampleRate = sampleRate;
    ctx.samplesPerBeat = samplesPerBeat (sampleRate, getTempo());
    enableRamp.setTime (sampleRate, kEnableRampSec);
    mixRamp.setTime (sampleRate, kMixRampSec);
    beatSamples.setTime (sampleRate / kChunk, kTempoRampSec);
    enableRamp.snap (on ? 1.0F : 0.0F);
    mixRamp.snap (getMix());
    beatSamples.snap (static_cast<float> (ctx.samplesPerBeat));
    effect->prepare (sampleRate, kChunk);
    idle = ! on;
    quietSamples = 0;
    active.store (on, kRelaxed);
}

void FxUnit::process (float* left, float* right, int numSamples) noexcept
{
    const juce::ScopedNoDenormals noDenormals;
    for (int offset = 0; offset < numSamples; offset += kChunk)
        processChunk (left + offset, right + offset, std::min (kChunk, numSamples - offset));
}

void FxUnit::processChunk (float* left, float* right, int n) noexcept
{
    const bool on = isEnabled();
    if (idle && ! on)
        return;
    if (idle)
    {
        idle = false;
        quietSamples = 0;
        active.store (true, kRelaxed);
    }
    ctx.samplesPerBeat = beatSamples.step (static_cast<float> (samplesPerBeat (ctx.sampleRate, getTempo())));
    computeGains (n, on);
    // Tail effects stop receiving input when disabled but keep ringing out.
    const bool tail = effect->hasTail();
    for (size_t i = 0; i < static_cast<size_t> (n); ++i)
    {
        const float send = tail ? onGain[i] : 1.0F;
        wetL[i] = left[i] * send;
        wetR[i] = right[i] * send;
    }
    effect->process (wetL.data(), wetR.data(), n, ctx);
    mixInto (left, right, n);
    updateIdle (n, on);
}

void FxUnit::computeGains (int n, bool on) noexcept
{
    const float enableTarget = on ? 1.0F : 0.0F, mix = getMix();
    for (size_t i = 0; i < static_cast<size_t> (n); ++i)
    {
        onGain[i] = enableRamp.step (enableTarget);
        mixGain[i] = mixRamp.step (mix);
    }
}

void FxUnit::mixInto (float* left, float* right, int n) const noexcept
{
    const bool tail = effect->hasTail();
    for (size_t i = 0; i < static_cast<size_t> (n); ++i)
    {
        const float g = onGain[i], m = mixGain[i];
        const float dry = 1.0F - m * g, wet = m * (tail ? 1.0F : g);
        left[i] = left[i] * dry + wetL[i] * wet;
        right[i] = right[i] * dry + wetR[i] * wet;
    }
}

float FxUnit::wetPeak (int n) const noexcept
{
    float peak = 0.0F;
    for (size_t i = 0; i < static_cast<size_t> (n); ++i)
        peak = std::max ({ peak, std::abs (wetL[i]), std::abs (wetR[i]) });
    return peak;
}

void FxUnit::updateIdle (int n, bool on) noexcept
{
    if (on || enableRamp.current() > 0.0F)
    {
        quietSamples = 0;
        return;
    }
    if (effect->hasTail())
    {
        quietSamples = wetPeak (n) < kSilence ? quietSamples + n : 0;
        if (quietSamples < effect->tailSilenceWindow (ctx))
            return;
    }
    effect->reset();
    idle = true;
    active.store (false, kRelaxed);
}

} // namespace openmixer::fx
