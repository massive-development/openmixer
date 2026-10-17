// Wraps an AudioCallback and counts bad output samples (NaN/Inf/overflow) on the audio thread.
#pragma once

#include "engine/AudioDevice.h"

#include <atomic>
#include <cmath>
#include <cstdint>

namespace openmixer
{

struct OutputReport
{
    std::uint64_t nonFinite = 0, overflow = 0;
    float peak = 0.0F;
};

class OutputChecker : public AudioCallback
{
public:
    static constexpr float kOverflowLimit = 4.0F;   // far beyond any legal master level

    explicit OutputChecker (AudioCallback& wrapped) : inner (wrapped) {}

    void prepare (double sampleRate, int maxBlockSize) override { inner.prepare (sampleRate, maxBlockSize); }
    void release() override { inner.release(); }

    void process (const float* const* inputs, int numInputs,
                  float* const* outputs, int numOutputs, int numSamples) override
    {
        inner.process (inputs, numInputs, outputs, numOutputs, numSamples);
        for (int c = 0; c < numOutputs; ++c)
            scan (outputs[c], numSamples);
    }

    OutputReport getReport() const noexcept
    {
        return { badValues.load (std::memory_order_relaxed), overflows.load (std::memory_order_relaxed),
                 peak.load (std::memory_order_relaxed) };
    }

private:
    void scan (const float* data, int n) noexcept
    {
        std::uint64_t bad = 0, over = 0;
        float blockPeak = 0.0F;
        for (int i = 0; i < n; ++i)
        {
            if (! std::isfinite (data[i])) { ++bad; continue; }
            const auto a = std::fabs (data[i]);
            over += a > kOverflowLimit ? 1U : 0U;
            blockPeak = a > blockPeak ? a : blockPeak;
        }
        badValues.fetch_add (bad, std::memory_order_relaxed);
        overflows.fetch_add (over, std::memory_order_relaxed);
        if (blockPeak > peak.load (std::memory_order_relaxed))
            peak.store (blockPeak, std::memory_order_relaxed);
    }

    AudioCallback& inner;
    std::atomic<std::uint64_t> badValues { 0 }, overflows { 0 };
    std::atomic<float> peak { 0.0F };
};

} // namespace openmixer
