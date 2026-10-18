#include "HotcuePads.h"

#include "Theme.h"

namespace openmixer::ui
{

namespace
{
    constexpr int kColumns = 4;
}

HotcuePads::HotcuePads (DeckBinding binding) : deckRef (binding)
{
    for (int i = 0; i < HotcueBank::kNumCues; ++i)
    {
        auto& p = pad (i);
        p.setButtonText (juce::String (i + 1));
        p.setClickingTogglesState (false);
        p.onClick = [this, i]
        {
            const auto mods = juce::ModifierKeys::currentModifiers;
            pressPad (i, mods.isShiftDown() || mods.isAltDown() || mods.isCtrlDown());
        };
        addAndMakeVisible (p);
    }
    applyTheme();
    refresh();
}

void HotcuePads::applyTheme()
{
    for (int i = 0; i < HotcueBank::kNumCues; ++i)
        pad (i).setColour (juce::TextButton::buttonOnColourId, theme::accentFor (i));
}

void HotcuePads::pressPad (int index, bool clear)
{
    if (! HotcueBank::inRange (index))
        return;
    deckRef.send ("hotcue" + juce::String (index) + (clear ? ".clear" : ""), 1.0);
    refresh();
}

void HotcuePads::refresh()
{
    for (int i = 0; i < HotcueBank::kNumCues; ++i)
        pad (i).setToggleState (deckRef.deck().getHotcue (i).has_value(), juce::dontSendNotification);
}

void HotcuePads::resized()
{
    const int rows = (HotcueBank::kNumCues + kColumns - 1) / kColumns, gap = theme::innerGap();
    const int w = (getWidth() - gap * (kColumns - 1)) / kColumns, h = (getHeight() - gap * (rows - 1)) / rows;
    for (int i = 0; i < HotcueBank::kNumCues; ++i)
        pad (i).setBounds ((i % kColumns) * (w + gap), (i / kColumns) * (h + gap), w, h);
}

} // namespace openmixer::ui
