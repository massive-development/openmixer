// Deck-level performance controls that need more than the Deck: sync, quantized loops, slip.
// Decorates a Deck as its ParameterTarget (router prefix "deck.<N>"): the ids below are handled
// here, everything else goes to Deck::setParameter, so existing maps keep working.
// Ids: sync (>=0.5 follow the leader), sync.master (>=0.5 make this deck the leader, <0.5 release),
//      loop.quantized.<beats> (>=0.5 loop <beats> from the nearest grid step), slip (>=0.5 on).
// Threads: control threads only (MIDI/UI/timer); the mutex guards slip state, never the audio thread.
#pragma once

#include "Deck.h"
#include "SlipTracker.h"
#include "Sync.h"

#include <memory>
#include <mutex>

namespace openmixer
{

class DeckControl : public ParameterTarget
{
public:
    DeckControl (Deck& deckToControl, Sync& syncEngine, int deckIndex) noexcept
        : deck (deckToControl), sync (syncEngine), index (deckIndex) {}

    bool setParameter (const juce::String& id, double value) override;

    // Plug-in point for the analysis pipeline; nullptr removes the grid (no sync/quantize).
    void setGrid (std::shared_ptr<const BeatGrid> grid) { sync.setGrid (index, std::move (grid)); }
    std::shared_ptr<const BeatGrid> getGrid() const { return sync.getGrid (index); }

    // False when there is no grid or the length is not positive.
    bool setQuantizedLoop (double lengthBeats);
    void setSlip (bool on);
    bool isSlipping() const;

    // Control-timer step: moves the slip ghost playhead by elapsed time at the current rate.
    void tick (double elapsedSeconds);

private:
    void setLeader (bool on);
    bool setSyncParameter (const juce::String& id, double value);
    bool setLoopParameter (const juce::String& id, double value);

    Deck& deck;
    Sync& sync;
    int index;
    mutable std::mutex slipMutex;
    SlipTracker slip;
};

} // namespace openmixer
