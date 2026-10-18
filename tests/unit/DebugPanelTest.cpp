// OpenMixer DebugPanelTest - the deck strip widgets (the debug console's mixer is covered by MixerWidgetTest)
#include <JuceHeader.h>
#include "ui/DeckStrip.h"
#include "engine/DjEngine.h"

#include <gtest/gtest.h>

TEST (DebugPanelTest, RefreshKeepsBoundsAndPositionReadoutWidthConstant)
{
    openmixer::DjEngine engine (2);
    openmixer::ui::DeckStrip deck (engine.getDeck (0), "Deck 1");
    deck.setSize (400, 96);
    juce::Array<juce::Rectangle<int>> before;
    for (auto* c : deck.getChildren())
        before.add (c->getBounds());
    for (int i = 0; i < 10; ++i)
        deck.refresh();
    for (int i = 0; i < deck.getNumChildComponents(); ++i)
        EXPECT_EQ (deck.getChildComponent (i)->getBounds(), before[i]) << "refresh moved a child";

    const auto shortText = openmixer::ui::DeckStrip::formatPosition (0.0, 0.0);
    const auto longText = openmixer::ui::DeckStrip::formatPosition (1111.1, 3599.9);
    EXPECT_EQ (shortText.length(), longText.length());
}
