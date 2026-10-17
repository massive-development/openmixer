// Decoder thread feeding a Deck's ring buffer and loop prefetch slots; owns the file reader.
#pragma once

#include "Deck.h"

namespace openmixer
{

class DeckDecoder : public juce::Thread
{
public:
    explicit DeckDecoder (Deck& owner);

    void run() override;

private:
    void takePendingReader();
    void syncTo (juce::uint32 epoch);
    void serviceRingRequest();
    void publishAck (juce::int64 target);
    void servicePrefetch();
    void fillPrefetch (juce::int64 start);
    bool canProduce() const;
    void produceChunk();
    void decodeAt (juce::int64 from, int numFrames, float* interleavedOut);

    Deck& deck;
    std::unique_ptr<juce::AudioFormatReader> reader;
    juce::AudioBuffer<float> tmp;
    std::vector<float> interleaved;
    juce::int64 readPos = 0;
    juce::int64 prefetchedStart = -1;
    juce::uint32 localAck = 0, localSeq = 0, localRingReq = 0, readerLoadId = 0;
};

} // namespace openmixer
