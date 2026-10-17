// Deck audio-thread key-lock path: source frames -> Rubber Band -> output, pitch held constant.
#include "Deck.h"

#include <algorithm>
#include <cmath>

namespace openmixer
{

namespace
{
    constexpr auto kRelaxed = std::memory_order_relaxed;
    constexpr double kMinKeylockRate = 0.05;   // below this the stretch ratio explodes; deck is silent
}

void Deck::enterKeylock() noexcept
{
    stretchDiscard = stretch.restart();
    feedNextFirst = primed;   // the interpolator already pulled this frame
    lastFedIdx = prevIdx;
    stretchActive = true;
}

void Deck::leaveKeylock() noexcept
{
    stretchActive = false;
    primed = haveFirst = false;   // re-prime from the next unread frame
}

bool Deck::feedStretch() noexcept
{
    const int need = std::max (1, stretch.getSamplesRequired());
    int got = 0;
    if (feedNextFirst)
    {
        stretchIn[0][0] = next[0];
        stretchIn[1][0] = next[1];
        lastFedIdx = nextIdx;
        got = 1;
        feedNextFirst = false;
    }
    float f[2];
    juce::int64 idx = 0;
    while (got < need && pullFrame (f, idx))
    {
        stretchIn[0][static_cast<size_t> (got)] = f[0];
        stretchIn[1][static_cast<size_t> (got)] = f[1];
        lastFedIdx = idx;
        ++got;
    }
    if (got == 0)
        return false;
    const float* in[2] { stretchIn[0].data(), stretchIn[1].data() };
    stretch.process (in, got);
    return true;
}

int Deck::drainStretch (float* left, float* right, int wanted) noexcept
{
    const int avail = stretch.getAvailable();
    if (stretchDiscard > 0)
    {
        const int k = std::min ({ avail, stretchDiscard, stretch.getMaxFeed() });
        float* sink[2] { stretchIn[0].data(), stretchIn[1].data() };
        stretchDiscard -= stretch.retrieve (sink, k);
        return 0;
    }
    float* dst[2] { left, right };
    return stretch.retrieve (dst, std::min (avail, wanted));
}

void Deck::renderKeylocked (float* left, float* right, int numSamples) noexcept
{
    const double r = rate.load (kRelaxed);
    if (r < kMinKeylockRate)
    {
        clearTail (left, right, 0, numSamples);
        return;
    }
    if (! stretchActive)
        enterKeylock();

    const double deviceOverFile = deviceSampleRate.load (kRelaxed) / fileSampleRate.load (kRelaxed);
    stretchRatio = deviceOverFile / r;
    stretch.setRatios (stretchRatio, 1.0 / deviceOverFile);

    int done = 0;
    while (done < numSamples)
    {
        if (stretch.getAvailable() > 0)
            done += drainStretch (left + done, right + done, numSamples - done);
        else if (! feedStretch())
            break;
    }
    if (done < numSamples)
    {
        noteStarved();
        clearTail (left, right, done, numSamples);
    }
    const auto latency = std::lround ((stretch.getStartDelay() + stretch.getAvailable()) / stretchRatio);
    positionFrames.store (std::max<juce::int64> (0, lastFedIdx - latency), kRelaxed);
}

} // namespace openmixer
