#include "FxRouting.h"

#include "Deck.h"

namespace openmixer
{

FxRouting::FxRouting (Mixer& mixer, int numChannels) : masterRack (std::make_unique<fx::FxRack>())
{
    for (int ch = 0; ch < numChannels; ++ch)
    {
        channelRacks.push_back (std::make_unique<fx::FxRack>());
        mixer.setChannelInsert (ch, kInsertSlot, channelRacks.back().get());
    }
    mixer.setMasterInsert (kInsertSlot, masterRack.get());
}

fx::FxRack* FxRouting::findRack (const juce::String& name) noexcept
{
    if (name == "master")
        return masterRack.get();
    const auto digits = name.substring (2);
    if (! name.startsWith ("ch") || digits.isEmpty() || ! digits.containsOnly ("0123456789"))
        return nullptr;
    const int ch = digits.getIntValue();
    return ch < static_cast<int> (channelRacks.size()) ? channelRacks[static_cast<size_t> (ch)].get() : nullptr;
}

bool FxRouting::setParameter (const juce::String& id, double value)
{
    const auto dot = id.indexOfChar ('.');
    auto* rack = dot > 0 ? findRack (id.substring (0, dot)) : nullptr;
    return rack != nullptr && rack->setParameter (id.substring (dot + 1), value);
}

int FxRouting::masterSourceDeck (const Sync& sync, int numDecks) const
{
    if (const int leader = sync.getLeader(); leader >= 0 && leader < numDecks && sync.getGrid (leader) != nullptr)
        return leader;
    for (int d = 0; d < numDecks; ++d)
        if (sync.getGrid (d) != nullptr)
            return d;
    return -1;
}

void FxRouting::followDecks (const std::vector<Deck*>& decks, const Sync& sync)
{
    const int count = static_cast<int> (decks.size());
    for (int d = 0; d < count && d < static_cast<int> (channelRacks.size()); ++d)
        if (const auto grid = sync.getGrid (d); grid != nullptr && decks[static_cast<size_t> (d)] != nullptr)
            channelRacks[static_cast<size_t> (d)]->setTempoFromGrid (*grid, decks[static_cast<size_t> (d)]->getRate());

    const int source = masterSourceDeck (sync, count);
    if (source >= 0 && decks[static_cast<size_t> (source)] != nullptr)
        masterRack->setTempoFromGrid (*sync.getGrid (source), decks[static_cast<size_t> (source)]->getRate());
}

} // namespace openmixer
