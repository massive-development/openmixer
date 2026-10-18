#include "TransportPanel.h"

#include "Theme.h"

namespace openmixer::ui
{

namespace
{
    struct Spec { const char* text; const char* id; bool toggle; const char* tip; };
    constexpr Spec kSpecs[TransportPanel::NumButtons] = {
        { "Play", "play", true, "Play / pause" },          { "Cue", "cue", false, "Stop and return to start" },
        { "Sync", "sync", true, "Follow the sync leader" }, { "Mstr", "sync.master", true, "Make this deck the sync master" },
        { "Key", "keylock", true, "Key lock: tempo changes keep the pitch" }, { "Quant", "quantize", true, "Quantize loop starts to the beat grid" },
        { "Slip", "slip", true, "Slip mode: the track keeps running under loops and scratches" },
    };
}

TransportPanel::TransportPanel (DeckBinding binding) : deckRef (binding)
{
    for (int i = 0; i < NumButtons; ++i)
    {
        auto& b = buttons[static_cast<size_t> (i)];
        b.setButtonText (kSpecs[i].text);
        b.setTooltip (kSpecs[i].tip);
        b.setClickingTogglesState (false);   // state mirrors the engine, not the click
        b.onClick = [this, i] { press (static_cast<Button> (i)); };
        addAndMakeVisible (b);
    }
    applyTheme();
    refresh();
}

void TransportPanel::applyTheme()
{
    for (auto& b : buttons)
        b.setColour (juce::TextButton::buttonOnColourId, theme::accentFor (deckRef.index));
    button (Master).setColour (juce::TextButton::buttonOnColourId, theme::master());
}

bool TransportPanel::stateOf (Button b) const
{
    switch (b)
    {
        case Play:     return deckRef.deck().isPlaying();
        case Sync:     return deckRef.sync().isEnabled (deckRef.index);
        case Master:   return deckRef.sync().getLeader() == deckRef.index;
        case KeyLock:  return deckRef.deck().isKeylock();
        case Quantize: return deckRef.control().isQuantize();
        case Slip:     return deckRef.control().isSlipping();
        default:       return false;
    }
}

void TransportPanel::press (Button b)
{
    if (b == Cue)
    {
        // No main-cue point exists in the engine yet: Cue stops and returns to the start.
        deckRef.send ("play", 0.0);
        deckRef.send ("seek", 0.0);
        return;
    }
    deckRef.send (kSpecs[b].id, stateOf (b) ? 0.0 : 1.0);
    refresh();
}

void TransportPanel::refresh()
{
    for (int i = 0; i < NumButtons; ++i)
        if (kSpecs[i].toggle)
            buttons[static_cast<size_t> (i)].setToggleState (stateOf (static_cast<Button> (i)), juce::dontSendNotification);
    button (Play).setButtonText (deckRef.deck().isPlaying() ? "Pause" : "Play");
}

void TransportPanel::resized()
{
    auto area = getLocalBounds();
    const int cell = (area.getWidth() - theme::innerGap() * (NumButtons - 1)) / NumButtons;
    for (auto& b : buttons)
    {
        b.setBounds (area.removeFromLeft (cell));
        area.removeFromLeft (theme::innerGap());
    }
}

} // namespace openmixer::ui
