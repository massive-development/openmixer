// Decks feeding the mixer; one AudioCallback for the whole signal path.
// Also owns per-channel and master FX racks, sync, and per-deck performance controls; the app
// registers getFx() ("fx"), getSync() ("sync") and getDeckControl(i) ("deck.<i>") with its router.
// Output channels: 0/1 master, 2/3 headphone cue (ignored on 2-channel devices).
#pragma once

#include "AudioDevice.h"
#include "Deck.h"
#include "DeckControl.h"
#include "FxRouting.h"
#include "Mixer.h"
#include "Sync.h"

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
    FxRouting& getFx() noexcept { return *fxRouting; }
    Sync& getSync() noexcept { return sync; }
    DeckControl& getDeckControl (int index) noexcept { return *controls[static_cast<size_t> (index)]; }

    // Beat grid for a deck (nullptr removes it); the analysis pipeline plugs in here. Control thread.
    void setGrid (int deck, std::shared_ptr<const BeatGrid> grid) { getDeckControl (deck).setGrid (std::move (grid)); }

    // Control-timer step (~20-50 Hz, control thread): sync rates/phase, FX tempo, slip ghosts.
    void updateControl (double elapsedSeconds);

    void prepare (double sampleRate, int maxBlockSize) override;
    void process (const float* const* inputs, int numInputs,
                  float* const* outputs, int numOutputs, int numSamples) override;

private:
    void renderBlock (float* const* outputs, int numOutputs, int offset, int n) noexcept;

    std::vector<std::unique_ptr<Deck>> decks;
    Mixer mixer;
    Sync sync;
    std::vector<Deck*> deckPointers;
    std::vector<std::unique_ptr<DeckControl>> controls;
    std::unique_ptr<FxRouting> fxRouting;   // after mixer: its racks are inserted into it
    int maxBlock = 0;
    std::vector<std::vector<float>> deckBuffers;   // L,R per deck
    std::vector<float> masterSink[2], cueSink[2];   // used when the device lacks those outputs
};

} // namespace openmixer
