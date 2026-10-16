// Decks feeding the mixer; one AudioCallback for the whole signal path.
// Output channels: 0/1 master, 2/3 headphone cue (ignored on 2-channel devices).
#pragma once

#include "AudioDevice.h"
#include "Deck.h"
#include "Mixer.h"

#include <memory>
#include <vector>

namespace openmixer
{

class DjEngine : public AudioCallback
{
public:
    explicit DjEngine (int numDecks = 2);

    int getNumDecks() const noexcept { return static_cast<int> (decks.size()); }
    Deck& getDeck (int index) noexcept { return *decks[static_cast<size_t> (index)]; }
    Mixer& getMixer() noexcept { return mixer; }

    void prepare (double sampleRate, int maxBlockSize) override;
    void process (const float* const* inputs, int numInputs,
                  float* const* outputs, int numOutputs, int numSamples) override;

private:
    void renderBlock (float* const* outputs, int numOutputs, int offset, int n) noexcept;

    std::vector<std::unique_ptr<Deck>> decks;
    Mixer mixer;
    int maxBlock = 0;
    std::vector<std::vector<float>> deckBuffers;   // L,R per deck
    std::vector<float> masterSink[2], cueSink[2];   // used when the device lacks those outputs
};

} // namespace openmixer
