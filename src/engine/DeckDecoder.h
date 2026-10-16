// Decoder thread feeding a Deck's ring buffer; owns the file reader.
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
    bool canProduce() const;
    void produceChunk();
    void decodeInto (int numFrames);

    Deck& deck;
    std::unique_ptr<juce::AudioFormatReader> reader;
    juce::AudioBuffer<float> tmp;
    std::vector<float> interleaved;
    juce::int64 readPos = 0;
    juce::uint32 localAck = 0;
};

} // namespace openmixer
