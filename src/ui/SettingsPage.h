// Base for one tab of the settings panel: a column of "label | control" rows (or full-width rows) laid
// out from theme tokens. Pages edit a Preferences value in place and call `changed` so the owner can
// persist and apply it. Message thread only.
#pragma once

#include "Preferences.h"

#include <juce_gui_basics/juce_gui_basics.h>

#include <memory>
#include <vector>

namespace openmixer::ui
{

class SettingsPage : public juce::Component
{
public:
    // Called after a page edited the preferences (the owner saves + applies).
    std::function<void()> changed;

    // Re-reads the stored values into the widgets without raising `changed`.
    virtual void reflect() {}

    void resized() override;
    void lookAndFeelChanged() override { applyTheme(); }

protected:
    void addRow (const juce::String& caption, juce::Component& control, int rows = 1);
    void addWide (juce::Component& control, int rows = 1);   // spans label + control columns
    juce::Label& addNote (const juce::String& text);          // small explanatory line
    void notifyChanged() { if (changed) changed(); }
    void applyTheme();

private:
    struct Row
    {
        std::unique_ptr<juce::Label> caption;   // null for full-width rows
        juce::Component* control = nullptr;
        int rows = 1;
        bool isNote = false;
    };
    std::vector<Row> items;
};

} // namespace openmixer::ui
