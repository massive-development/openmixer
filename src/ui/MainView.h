// Main window content: deck widgets and named slots arranged by a LayoutSpec on the glass backdrop.
// The mixer and library are not built here: the "mixer" and "library" slots hold placeholders until
// the app (or a plugin) calls setSlotContent(). Message thread only.
#pragma once

#include "DeckWidget.h"
#include "MainLayout.h"

#include <juce_gui_basics/juce_gui_basics.h>

#include <map>
#include <memory>
#include <vector>

namespace openmixer::ui
{

// Stand-in drawn in an empty slot so the layout is visible before its real component exists.
class SlotPlaceholder : public juce::Component
{
public:
    explicit SlotPlaceholder (juce::String slotName) : name (std::move (slotName)) {}
    void paint (juce::Graphics& g) override;

private:
    juce::String name;
};

class MainView : public juce::Component
{
public:
    MainView (DjEngine& engine, ParameterTarget& target, LayoutSpec layout);
    MainView (DjEngine& engine, ParameterTarget& target) : MainView (engine, target, LayoutSpec::standard (engine.getNumDecks())) {}

    DeckWidget* getDeck (int index) noexcept;
    int getNumDecks() const noexcept { return static_cast<int> (decks.size()); }

    // Replaces the placeholder (or previous content) of a slot; returns false for an unknown slot name.
    bool setSlotContent (const juce::String& slot, std::unique_ptr<juce::Component> content);
    juce::Component* getSlot (const juce::String& slot) noexcept;

    // 1 = solid backdrop; below 1 the desktop shows through a semi-transparent window.
    void setBackdropOpacity (float opacity);
    const LayoutSpec& getLayout() const noexcept { return spec; }

    void paint (juce::Graphics& g) override;
    void resized() override;

private:
    LayoutSpec spec;
    std::vector<std::unique_ptr<DeckWidget>> decks;   // indexed by deck number
    std::map<juce::String, std::unique_ptr<juce::Component>> slots;
    float backdropOpacity = 1.0F;
};

} // namespace openmixer::ui
