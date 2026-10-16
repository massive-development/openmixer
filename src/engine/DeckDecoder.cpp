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
        deck.hasPendingReader = false;
    }
}

void DeckDecoder::syncTo (juce::uint32 epoch)
{
    const juce::int64 len = reader != nullptr ? reader->lengthInSamples : 0;
    readPos = std::clamp<juce::int64> (deck.seekTarget.load (kRelaxed), 0, len);
    deck.ackTarget.store (readPos, kRelaxed);
    deck.endOfStream.store (reader == nullptr || readPos >= len, kRelaxed);
    localAck = epoch;
    deck.ackEpoch.store (epoch, kRelease);
}

bool DeckDecoder::canProduce() const
{
    return reader != nullptr
        && deck.drainedEpoch.load (kAcquire) == localAck
        && readPos < reader->lengthInSamples
        && deck.ring.getWritableCount() >= static_cast<size_t> (kChunkFrames) * 2;
}

void DeckDecoder::produceChunk()
{
    const auto n = static_cast<int> (std::min<juce::int64> (kChunkFrames, reader->lengthInSamples - readPos));
    decodeInto (n);
    deck.ring.write (interleaved.data(), static_cast<size_t> (n) * 2);
    readPos += n;
    if (readPos >= reader->lengthInSamples)
        deck.endOfStream.store (true, kRelease);
}

void DeckDecoder::decodeInto (int n)
{
    const auto fileChans = static_cast<int> (std::min<unsigned int> (reader->numChannels, 2U));
    tmp.clear();
    reader->read (tmp.getArrayOfWritePointers(), fileChans, readPos, n);
    if (fileChans == 1)
        tmp.copyFrom (1, 0, tmp, 0, 0, n);

    const float* l = tmp.getReadPointer (0);
    const float* r = tmp.getReadPointer (1);
    for (int i = 0; i < n; ++i)
    {
        interleaved[static_cast<size_t> (i) * 2] = l[i];
        interleaved[static_cast<size_t> (i) * 2 + 1] = r[i];
    }
}

} // namespace openmixer
