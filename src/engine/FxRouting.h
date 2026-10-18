// FX racks for the signal path: one per mixer channel plus one on master, each inserted at
// slot 0 of its StageChain. One ParameterTarget (router prefix "fx") reaches all of them.
// Ids: master.<rack id> and ch<N>.<rack id>, where <rack id> is an FxRack id ("bpm",
// "fx<N>.enable", "fx<N>.mix", "fx<N>.<effect param>"), e.g. "master.fx1.mix", "ch0.fx2.enable".
// Threads: control thread only (the racks themselves are audio-safe once inserted).
#pragma once

#include "Mixer.h"
#include "Sync.h"
#include "fx/FxRack.h"

#include <memory>
#include <vector>

namespace openmixer
{

class Deck;

class FxRouting : public ParameterTarget
{
public:
    static constexpr int kInsertSlot = 0;

    FxRouting (Mixer& mixer, int numChannels);

    bool setParameter (const juce::String& id, double value) override;

    fx::FxRack& getChannelRack (int ch) noexcept { return *channelRacks[static_cast<size_t> (ch)]; }
    fx::FxRack& getMasterRack() noexcept { return *masterRack; }

    // Feeds each rack the tempo of its deck (grid bpm x rate); master follows the sync leader,
    // else the first deck that has a grid. Racks without a grid keep their last tempo.
    // A deck with a grid overwrites a manually set fx.ch<N>.bpm on every call (~33 ms).
    void followDecks (const std::vector<Deck*>& decks, const Sync& sync);

private:
    fx::FxRack* findRack (const juce::String& name) noexcept;
    int masterSourceDeck (const Sync& sync, int numDecks) const;

    std::vector<std::unique_ptr<fx::FxRack>> channelRacks;
    std::unique_ptr<fx::FxRack> masterRack;
};

} // namespace openmixer
