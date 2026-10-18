// Settings tab: which MIDI inputs feed the MidiMapper and which controller map file it loads.
// Inputs come from a provider (JUCE's device list in the app, a fake in tests). Message thread only.
#pragma once

#include "PathRow.h"
#include "SettingsPage.h"

#include <juce_audio_devices/juce_audio_devices.h>

namespace openmixer::ui
{

class MidiSettingsPage : public SettingsPage
{
public:
    using InputsProvider = std::function<juce::Array<juce::MidiDeviceInfo>()>;

    MidiSettingsPage (Preferences& prefs, InputsProvider inputs);

    void reflect() override;
    void setUseAll (bool useAll);                               // what the "all inputs" switch does
    void setInputEnabled (const juce::String& identifier, bool enabled);
    void setMapFile (const juce::File& file);                    // what "Choose..." does
    void useDefaultMap();

    void resized() override;

    juce::ToggleButton& getUseAllToggle() noexcept { return useAll_; }
    int getNumInputRows() const noexcept { return list.getNumRows(); }
    juce::ToggleButton* getInputToggle (int row) noexcept { return list.getToggle (row); }
    PathRow& getMapRow() noexcept { return mapRow; }

private:
    class InputList : public juce::Component
    {
    public:
        void setRows (const juce::Array<juce::MidiDeviceInfo>& devices, const juce::StringArray& enabled, bool all,
                      std::function<void (const juce::String&, bool)> onToggle);
        int getNumRows() const noexcept { return toggles.size(); }
        juce::ToggleButton* getToggle (int row) { return toggles[row]; }
        int getContentHeight() const;
        void resized() override;

    private:
        juce::OwnedArray<juce::ToggleButton> toggles;
    };

    Preferences& prefs;
    InputsProvider inputs;
    juce::ToggleButton useAll_ { "Use all MIDI inputs" };
    juce::Viewport viewport;
    InputList list;
    PathRow mapRow { "Choose a controller map", "*.json" };
    juce::Label emptyNote;
};

} // namespace openmixer::ui
