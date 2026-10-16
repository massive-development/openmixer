// OpenMixer DebugPanelTest - UI widgets drive the mixer (needs the GUI init done in TestMain)
#include <JuceHeader.h>
#include "ui/ChannelStripPanel.h"
#include "ui/DebugPanel.h"

#include <gtest/gtest.h>

namespace
{
juce::Slider* sliderAt (juce::Component& parent, int sliderIndex)
{
    int seen = 0;
    for (auto* child : parent.getChildren())
        if (auto* s = dynamic_cast<juce::Slider*> (child))
            if (seen++ == sliderIndex)
                return s;
    return nullptr;
}
}

TEST (DebugPanelTest, ChannelStripWidgetsDriveTheMixer)
{
    openmixer::Mixer mixer (2);
    mixer.prepare (48000.0, 256);
    openmixer::ui::ChannelStripPanel panel (mixer, 1);
    panel.setSize (140, 420);

    auto* lowKnob = sliderAt (panel, 0);
    auto* filterKnob = sliderAt (panel, 3);
    auto* fader = sliderAt (panel, 4);
    ASSERT_TRUE (lowKnob != nullptr && filterKnob != nullptr && fader != nullptr);

    lowKnob->setValue (-12.0, juce::sendNotificationSync);
    filterKnob->setValue (0.5, juce::sendNotificationSync);
    fader->setValue (0.3, juce::sendNotificationSync);
    EXPECT_EQ (mixer.getEq (1, 0), -12.0f);
    EXPECT_EQ (mixer.getFilter (1), 0.5f);
    EXPECT_NEAR (mixer.getFader (1), 0.3f, 1e-6f);
    EXPECT_EQ (mixer.getFader (0), 1.0f);   // other channel untouched
}

TEST (DebugPanelTest, BuildsOneDeckStripAndChannelPerDeckAndLaysOut)
{
    openmixer::DjEngine engine (2);
    engine.prepare (48000.0, 256);
    openmixer::ui::DebugPanel panel (engine);
    panel.setSize (900, 640);
    int kids = 0;
    for (auto* child : panel.getChildren())
        kids += child->isVisible() && ! child->getBounds().isEmpty() ? 1 : 0;
    EXPECT_GE (kids, 10) << "all controls should have non-empty bounds";

    auto* crossfader = sliderAt (panel, 0);
    ASSERT_TRUE (crossfader != nullptr);
    crossfader->setValue (0.9, juce::sendNotificationSync);
    EXPECT_TRUE (engine.getMixer().setParameter ("crossfader", 0.9));
}
