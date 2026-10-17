// Deck audio-thread side: handshake servicing, frame sourcing (ring or loop slot) and interpolation.
#include "Deck.h"

#include <algorithm>

namespace openmixer
{

namespace
{
    constexpr auto kRelaxed = std::memory_order_relaxed;
    constexpr auto kAcquire = std::memory_order_acquire;
    constexpr auto kRelease = std::memory_order_release;
}

void Deck::clearTail (float* left, float* right, int from, int numSamples) const noexcept
{
    if (from >= numSamples)
        return;
    juce::FloatVectorOperations::clear (left + from, numSamples - from);
    juce::FloatVectorOperations::clear (right + from, numSamples - from);
}

bool Deck::serviceHandshake() noexcept
{
    const auto req = requestEpoch.load (kAcquire);
    const auto ack = ackEpoch.load (kAcquire);
    if (ack != req)
        return false;

    const auto seq = ackSeq.load (kAcquire);
    if (drainedSeq.load (kRelaxed) != seq)
    {
        ring.skip (ring.getReadableCount());   // everything before the ack is stale
        ringIdx = ackTarget.load (kRelaxed);
        drainedSeq.store (seq, kRelease);
    }
    if (ack != lastControlAck)
        resetPlayhead (ack);
    return true;
}

void Deck::resetPlayhead (juce::uint32 ack) noexcept
{
    lastControlAck = ack;
    playingLoadId = ackLoadId.load (kRelaxed);
    readIdx = ringWanted = prevIdx = nextIdx = ringIdx;
    primed = haveFirst = false;
    stretchActive = false;   // a seek restarts the stretcher too
    phase = 0.0;
    releaseSlot();
    positionFrames.store (ringIdx, kRelaxed);
}

bool Deck::reachedEnd() noexcept
{
    // Re-check readability after seeing end-of-stream: the producer's last write may race.
    if (slot == nullptr && ringIdx == readIdx && endOfStream.load (kAcquire) && ring.getReadableCount() < 2)
    {
        playing.store (false, kRelaxed);
        finished.store (true, kRelaxed);
        return true;
    }
    return false;
}

void Deck::noteStarved() noexcept
{
    if (! reachedEnd())
        underruns.fetch_add (1, kRelaxed);
}

//==============================================================================
void Deck::releaseSlot() noexcept
{
    if (slot != nullptr)
        prefetch.release();
    slot = nullptr;
}

void Deck::requestRing (juce::int64 target) noexcept
{
    if (ringWanted == target)
        return;
    ringWanted = target;
    ringReqTarget.store (target, kRelaxed);
    ringReqBase.store (lastControlAck, kRelaxed);
    ringReqEpoch.fetch_add (1, kRelease);
}

void Deck::enterLoopSlot() noexcept
{
    const auto usable = [this] (const LoopPrefetch::Slot* s)
    {
        return s != nullptr && s->loadId == playingLoadId && s->start == loopSnap.start;
    };
    if (! usable (slot))
    {
        slot = prefetch.acquire();
        if (! usable (slot))
        {
            releaseSlot();   // not prefetched yet: fall back to a (non-gapless) ring reposition
            requestRing (readIdx);
            return;
        }
    }
    if (! slot->contains (readIdx))
    {
        releaseSlot();
        requestRing (readIdx);
        return;
    }
    requestRing (slot->start + slot->length);
}

bool Deck::readSlotFrame (float* out) noexcept
{
    if (slot == nullptr)
        return false;
    if (! slot->contains (readIdx))
    {
        releaseSlot();
        return false;
    }
    const auto i = static_cast<size_t> (readIdx - slot->start) * 2;
    out[0] = slot->frames[i];
    out[1] = slot->frames[i + 1];
    return true;
}

bool Deck::readRingFrame (float* out) noexcept
{
    if (ringIdx != readIdx)
    {
        requestRing (readIdx);
        return false;
    }
    if (ring.read (out, 2) < 2)
        return false;
    ringWanted = ++ringIdx;   // reading moves the ring: a later request to an old target is new again
    return true;
}

bool Deck::pullFrame (float* out, juce::int64& index) noexcept
{
    if (loopSnap.active() && readIdx >= loopSnap.end && readIdx > loopSnap.start)
    {
        // Modulo keeps beat phase when a loop shrinks under the playhead.
        readIdx = loopSnap.start + (readIdx - loopSnap.start) % (loopSnap.end - loopSnap.start);
        enterLoopSlot();
    }
    if (! readSlotFrame (out) && ! readRingFrame (out))
        return false;
    index = readIdx++;
    return true;
}

//==============================================================================
bool Deck::popFrame() noexcept
{
    float f[2];
    juce::int64 idx = 0;
    if (! pullFrame (f, idx))
        return false;
    prev[0] = next[0]; prev[1] = next[1];
    next[0] = f[0];    next[1] = f[1];
    prevIdx = nextIdx;
    nextIdx = idx;
    return true;
}

bool Deck::primeInterpolator() noexcept
{
    if (! haveFirst)
    {
        if (! pullFrame (next, nextIdx))
            return false;
        haveFirst = true;
    }
    if (! popFrame())
        return false;
    primed = true;
    phase = 0.0;
    return true;
}

void Deck::renderStereo (float* left, float* right, int numSamples) noexcept
{
    const bool running = serviceHandshake() && playing.load (kRelaxed) && loaded.load (kRelaxed);
    if (! running)
    {
        clearTail (left, right, 0, numSamples);
        return;
    }

    loopState.load (loopSnap);   // keeps the previous snapshot while a writer is mid-update
    if (keylock.load (kRelaxed) && stretch.isPrepared())
    {
        renderKeylocked (left, right, numSamples);
        return;
    }
    if (stretchActive)
        leaveKeylock();
    renderInterpolated (left, right, numSamples);
}

void Deck::renderInterpolated (float* left, float* right, int numSamples) noexcept
{
    if (! primed && ! primeInterpolator())
    {
        noteStarved();
        clearTail (left, right, 0, numSamples);
        return;
    }

    const double ratio = rate.load (kRelaxed) * (fileSampleRate.load (kRelaxed) / deviceSampleRate.load (kRelaxed));
    for (int i = 0; i < numSamples; ++i)
    {
        while (phase >= 1.0)
        {
            if (! popFrame())
            {
                noteStarved();
                clearTail (left, right, i, numSamples);
                positionFrames.store (prevIdx, kRelaxed);
                return;
            }
            phase -= 1.0;
        }
        const auto t = static_cast<float> (phase);
        left[i]  = prev[0] + (next[0] - prev[0]) * t;
        right[i] = prev[1] + (next[1] - prev[1]) * t;
        phase += ratio;
    }
    positionFrames.store (prevIdx, kRelaxed);
}

} // namespace openmixer
