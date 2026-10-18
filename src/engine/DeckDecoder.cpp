#include "DeckDecoder.h"

#include <algorithm>

namespace openmixer
{

namespace
{
    constexpr int kChunkFrames = 2048;
    constexpr auto kRelaxed = std::memory_order_relaxed;
    constexpr auto kAcquire = std::memory_order_acquire;
    constexpr auto kRelease = std::memory_order_release;
}

DeckDecoder::DeckDecoder (Deck& owner)
    : juce::Thread ("OpenMixer Deck Decoder"), deck (owner),
      tmp (2, kChunkFrames), interleaved (static_cast<size_t> (kChunkFrames) * 2)
{}

void DeckDecoder::run()
{
    while (! threadShouldExit())
    {
        // Epoch first, reader second: a reader is published before the epoch bump naming it.
        const auto epoch = deck.requestEpoch.load (kAcquire);
        takePendingReader();
        if (epoch != localAck)
            syncTo (epoch);
        else
            serviceRingRequest();
        servicePrefetch();

        if (canProduce())
            produceChunk();
        else
            wait (3);
    }
}

void DeckDecoder::takePendingReader()
{
    const std::lock_guard<std::mutex> lock (deck.pendingMutex);
    if (deck.hasPendingReader)
    {
        reader = std::move (deck.pendingReader);
        readerLoadId = deck.pendingLoadId;
        deck.hasPendingReader = false;
        deck.prefetch.withdraw();
        prefetchedStart = -1;
    }
}

void DeckDecoder::publishAck (juce::int64 target)
{
    const juce::int64 len = reader != nullptr ? reader->lengthInSamples : 0;
    readPos = std::clamp<juce::int64> (target, 0, len);
    deck.ackTarget.store (readPos, kRelaxed);
    deck.endOfStream.store (reader == nullptr || readPos >= len, kRelaxed);
    deck.ackSeq.store (++localSeq, kRelease);
}

void DeckDecoder::syncTo (juce::uint32 epoch)
{
    publishAck (deck.seekTarget.load (kRelaxed));
    deck.ackLoadId.store (readerLoadId, kRelaxed);
    localAck = epoch;
    deck.ackEpoch.store (epoch, kRelease);   // after ackSeq: the audio thread reads the epoch first
}

void DeckDecoder::serviceRingRequest()
{
    const auto req = deck.ringReqEpoch.load (kAcquire);
    if (req == localRingReq || deck.drainedSeq.load (kAcquire) != localSeq)
        return;
    localRingReq = req;
    if (deck.ringReqBase.load (kRelaxed) == localAck)   // else it predates the latest control seek
        publishAck (deck.ringReqTarget.load (kRelaxed));
}

void DeckDecoder::servicePrefetch()
{
    LoopSnapshot loop;
    if (reader == nullptr || ! deck.loopState.load (loop) || ! loop.valid() || loop.start == prefetchedStart)
        return;
    fillPrefetch (loop.start);
}

void DeckDecoder::fillPrefetch (juce::int64 start)
{
    auto& slot = deck.prefetch.beginFill();
    const auto length = std::clamp<juce::int64> (reader->lengthInSamples - start, 0, deck.prefetch.getCapacityFrames());
    for (juce::int64 done = 0; done < length; done += kChunkFrames)
    {
        const auto n = static_cast<int> (std::min<juce::int64> (kChunkFrames, length - done));
        decodeAt (start + done, n, slot.frames.data() + done * 2);
    }
    slot.start = start;
    slot.length = length;
    slot.loadId = readerLoadId;
    deck.prefetch.publish (slot);
    prefetchedStart = start;
}

bool DeckDecoder::canProduce() const
{
    return reader != nullptr
        && deck.drainedSeq.load (kAcquire) == localSeq
        && readPos < reader->lengthInSamples
        && deck.ring.getWritableCount() >= static_cast<size_t> (kChunkFrames) * 2;
}

void DeckDecoder::produceChunk()
{
    const auto n = static_cast<int> (std::min<juce::int64> (kChunkFrames, reader->lengthInSamples - readPos));
    decodeAt (readPos, n, interleaved.data());
    deck.ring.write (interleaved.data(), static_cast<size_t> (n) * 2);
    readPos += n;
    if (readPos >= reader->lengthInSamples)
        deck.endOfStream.store (true, kRelease);
}

void DeckDecoder::decodeAt (juce::int64 from, int n, float* out)
{
    const auto fileChans = static_cast<int> (std::min<unsigned int> (reader->numChannels, 2U));
    tmp.clear();
    reader->read (tmp.getArrayOfWritePointers(), fileChans, from, n);
    if (fileChans == 1)
        tmp.copyFrom (1, 0, tmp, 0, 0, n);

    const float* l = tmp.getReadPointer (0);
    const float* r = tmp.getReadPointer (1);
    for (int i = 0; i < n; ++i)
    {
        out[static_cast<size_t> (i) * 2] = l[i];
        out[static_cast<size_t> (i) * 2 + 1] = r[i];
    }
}

} // namespace openmixer
