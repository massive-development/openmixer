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
    constexpr int kPrimeChunk = 512;           // extra frames per block while priming: bounds the CPU cost
}

//==============================================================================
void Deck::renderKeylocked (float* left, float* right, int numSamples) noexcept
{
    const double r = rate.load (kRelaxed);
    if (r < kMinKeylockRate)
    {
        clearTail (left, right, 0, numSamples);
        return;
    }
    const double deviceOverFile = deviceSampleRate.load (kRelaxed) / fileSampleRate.load (kRelaxed);
    stretchRatio = deviceOverFile / r;
    stretch.setRatios (stretchRatio, 1.0 / deviceOverFile);

    if (stretchMode == StretchMode::off)
        beginPriming();
    if (stretchMode == StretchMode::priming)
    {
        renderInterpolated (left, right, numSamples);
        stepPriming();
        return;
    }
    renderStretched (left, right, numSamples);
}

void Deck::renderStretched (float* left, float* right, int numSamples) noexcept
{
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
    positionFrames.store (std::max<juce::int64> (0, lastFedIdx - stretchLatencyFrames()), kRelaxed);
}

juce::int64 Deck::stretchLatencyFrames() const noexcept
{
    return std::lround ((stretch.getStartDelay() + stretch.getAvailable()) / stretchRatio);
}

//==============================================================================
void Deck::beginPriming() noexcept
{
    stretch.reset();
    // Buffering the pad is cheap; the real work happens per hop as frames arrive, which priming spreads out.
    for (int pad = stretch.getStartPad(); pad > 0; pad -= stretch.getMaxFeed())
        stretch.processSilence (std::min (pad, stretch.getMaxFeed()));
    stretchDiscard = stretch.getStartDelay();
    alignedSource = 0.0;
    teeCount = 0;
    primingPops = 0;
    stretchMode = StretchMode::priming;
    if (primed)
        teeFrame (next, nextIdx);   // pulled already but not yet heard
}

void Deck::stepPriming() noexcept
{
    pullAhead (kPrimeChunk);
    flushTee();
    if (discardTowardsAlignment())
    {
        stretchMode = StretchMode::on;
        lookahead.clear();   // already fed to the stretcher
    }
}

void Deck::pullAhead (int numFrames) noexcept
{
    float f[2];
    juce::int64 idx = 0;
    for (int i = 0; i < numFrames && lookahead.size() < lookahead.capacity() && pullFrame (f, idx); ++i)
    {
        lookahead.push (f, idx);
        teeFrame (f, idx);
    }
}

// Drops stretcher output until it lines up with the interpolator's playhead; true once aligned.
bool Deck::discardTowardsAlignment() noexcept
{
    const double target = static_cast<double> (primingPops - 1) + phase;
    for (int avail = stretch.getAvailable(); avail > 0; avail = stretch.getAvailable())
    {
        if (stretchDiscard > 0)
        {
            stretchDiscard -= retrieveToSink (std::min (avail, stretchDiscard));
            continue;
        }
        const double behind = target - alignedSource;
        if (behind <= 0.0)
            break;
        const int k = std::min (avail, static_cast<int> (std::ceil (behind * stretchRatio)));
        alignedSource += retrieveToSink (k) / stretchRatio;
    }
    return stretchDiscard == 0 && alignedSource >= target;
}

int Deck::retrieveToSink (int numFrames) noexcept
{
    float* sink[2] { stretchSink[0].data(), stretchSink[1].data() };
    return stretch.retrieve (sink, std::min (numFrames, stretch.getMaxFeed()));
}

void Deck::leaveKeylock() noexcept
{
    if (stretchMode == StretchMode::on)
    {
        // Replay what the stretcher had taken in but not yet played, so leaving skips nothing.
        history.copyNewestTo (lookahead, static_cast<size_t> (std::max<juce::int64> (0, stretchLatencyFrames())));
        primed = haveFirst = false;
    }
    stretchMode = StretchMode::off;
}

//==============================================================================
void Deck::teeFrame (const float* frame, juce::int64 index) noexcept
{
    if (teeCount == stretch.getMaxFeed())
        flushTee();
    stretchIn[0][static_cast<size_t> (teeCount)] = frame[0];
    stretchIn[1][static_cast<size_t> (teeCount)] = frame[1];
    ++teeCount;
    lastFedIdx = index;
    history.push (frame, index);
}

void Deck::flushTee() noexcept
{
    if (teeCount == 0)
        return;
    const float* in[2] { stretchIn[0].data(), stretchIn[1].data() };
    stretch.process (in, teeCount);
    teeCount = 0;
}

bool Deck::feedStretch() noexcept
{
    const int need = std::max (1, stretch.getSamplesRequired());
    float f[2];
    juce::int64 idx = 0;
    while (teeCount < need && nextSourceFrame (f, idx))
        teeFrame (f, idx);
    if (teeCount == 0)
        return false;
    flushTee();
    return true;
}

int Deck::drainStretch (float* left, float* right, int wanted) noexcept
{
    float* dst[2] { left, right };
    return stretch.retrieve (dst, std::min (stretch.getAvailable(), wanted));
}

} // namespace openmixer
