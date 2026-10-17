#include "StageCatalog.h"

#include <algorithm>

namespace openmixer
{

bool StageCatalog::add (const juce::String& owner, const juce::String& stageId, StageKind kind, StageFactory factory)
{
    if (stageId.isEmpty() || ! factory || contains (stageId))
        return false;
    factories.push_back ({ stageId, owner, kind, std::move (factory) });
    return true;
}

bool StageCatalog::contains (const juce::String& stageId) const
{
    return find (stageId) != nullptr;
}

const StageCatalog::Factory* StageCatalog::find (const juce::String& stageId) const
{
    const auto it = std::find_if (factories.begin(), factories.end(),
                                  [&] (const Factory& f) { return f.id == stageId; });
    return it == factories.end() ? nullptr : &*it;
}

juce::StringArray StageCatalog::getIds (const StageKind* kind) const
{
    juce::StringArray ids;
    for (const auto& f : factories)
        if (kind == nullptr || f.kind == *kind)
            ids.add (f.id);
    return ids;
}

bool StageCatalog::insert (Mixer& mixer, int channel, int slot, ProcessStage* stage)
{
    if (channel == kMasterChannel)
        return mixer.setMasterInsert (slot, stage);
    if (channel < 0 || channel >= mixer.getNumChannels())
        return false;
    return mixer.setChannelInsert (channel, slot, stage);
}

juce::Result StageCatalog::attach (const juce::String& stageId, Mixer& mixer, int channel, int slot)
{
    const auto* factory = find (stageId);
    if (factory == nullptr)
        return juce::Result::fail ("Unknown stage " + stageId);

    auto stage = factory->make();
    if (stage == nullptr || ! insert (mixer, channel, slot, stage.get()))
        return juce::Result::fail ("Cannot insert " + stageId + " at that channel/slot");

    const auto owner = factory->owner;
    const auto it = std::find_if (live.begin(), live.end(), [&] (const Live& l)
                                  { return l.mixer == &mixer && l.channel == channel && l.slot == slot; });
    if (it == live.end())
        live.push_back ({ &mixer, channel, slot, owner, std::move (stage) });
    else
        *it = { &mixer, channel, slot, owner, std::move (stage) };   // mixer already waited out the old pointer
    return juce::Result::ok();
}

std::vector<StageCatalog::Live>::iterator StageCatalog::remove (std::vector<Live>::iterator it)
{
    insert (*it->mixer, it->channel, it->slot, nullptr);
    return live.erase (it);
}

void StageCatalog::detach (Mixer& mixer, int channel, int slot)
{
    const auto it = std::find_if (live.begin(), live.end(), [&] (const Live& l)
                                  { return l.mixer == &mixer && l.channel == channel && l.slot == slot; });
    if (it != live.end())
        remove (it);
}

void StageCatalog::removeOwner (const juce::String& owner)
{
    for (auto it = live.begin(); it != live.end();)
    {
        it = it->owner == owner ? remove (it) : it + 1;
    }
    factories.erase (std::remove_if (factories.begin(), factories.end(),
                                     [&] (const Factory& f) { return f.owner == owner; }), factories.end());
}

} // namespace openmixer
