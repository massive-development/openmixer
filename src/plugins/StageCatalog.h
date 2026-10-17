// Plugin-provided stage factories plus the live instances inserted into a Mixer.
// Control thread only. Mixers passed to attach() must outlive their attachments.
#pragma once

#include "PluginApi.h"

#include "../engine/Mixer.h"

#include <memory>
#include <vector>

namespace openmixer
{

class StageCatalog
{
public:
    static constexpr int kMasterChannel = -1;

    bool add (const juce::String& owner, const juce::String& stageId, StageKind kind, StageFactory factory);
    bool contains (const juce::String& stageId) const;
    juce::StringArray getIds (const StageKind* kind = nullptr) const;

    // Creates a stage and inserts it; an existing stage in that slot is replaced and destroyed.
    juce::Result attach (const juce::String& stageId, Mixer& mixer, int channel, int slot);
    void detach (Mixer& mixer, int channel, int slot);

    // Detaches live instances and drops factories belonging to a plugin.
    void removeOwner (const juce::String& owner);

private:
    struct Factory { juce::String id, owner; StageKind kind; StageFactory make; };
    struct Live { Mixer* mixer; int channel; int slot; juce::String owner; std::unique_ptr<ProcessStage> stage; };

    static bool insert (Mixer& mixer, int channel, int slot, ProcessStage* stage);
    const Factory* find (const juce::String& stageId) const;
    std::vector<Live>::iterator remove (std::vector<Live>::iterator it);

    std::vector<Factory> factories;
    std::vector<Live> live;
};

} // namespace openmixer
