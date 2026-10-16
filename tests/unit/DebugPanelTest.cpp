// OpenMixer DebugPanelTest - UI widgets drive the mixer (needs the GUI init done in TestMain)
#include <JuceHeader.h>
#include "ui/ChannelStripPanel.h"
#include "ui/DebugPanel.h"
#include "ui/DeckStrip.h"

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

juce::StringArray labelTexts (juce::Component& parent)
{
    juce::StringArray texts;
    for (auto* child : parent.getChildren())
        if (auto* l = dynamic_cast<juce::Label*> (child))
            texts.add (l->getText());
    return texts;
}

// Every child must be non-empty, inside the parent, and clear of its siblings
void expectCleanLayout (juce::Component& parent, const juce::String& context)
{
    const auto bounds = parent.getLocalBounds();
    for (int i = 0; i < parent.getNumChildComponents(); ++i)
    {
        auto* a = parent.getChildComponent (i);
        EXPECT_TRUE (! a->getBounds().isEmpty() && bounds.contains (a->getBounds()))
            << context << ": child " << i << " clipped";
        for (int j = i + 1; j < parent.getNumChildComponents(); ++j)
            EXPECT_FALSE (a->getBounds().intersects (parent.getChildComponent (j)->getBounds()))
                << context << ": children " << i << "/" << j << " overlap";
    }
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

TEST (DebugPanelTest, EveryMixerControlHasACaption)
{
    openmixer::Mixer mixer (2);
    openmixer::ui::ChannelStripPanel strip (mixer, 0);
    strip.setSize (300, 440);
    const auto texts = labelTexts (strip);
    for (const char* name : { "Hi", "Mid", "Lo", "Trim", "Filter", "Fader", "X-fader", "Lvl", "Channel 1" })
        EXPECT_TRUE (texts.contains (name)) << "missing channel caption " << name;

    openmixer::DjEngine engine (2);
    openmixer::ui::DebugPanel panel (engine);
    panel.setSize (900, 640);
    const auto panelTexts = labelTexts (panel);
    for (const char* name : { "Crossfader", "Master", "Cue mix", "Cue level" })
        EXPECT_TRUE (panelTexts.contains (name)) << "missing panel caption " << name;
}

TEST (DebugPanelTest, LayoutIsClippedFreeAndOverlapFreeAcrossSizes)
{
    openmixer::DjEngine engine (2);
    openmixer::ui::DebugPanel panel (engine);
    for (auto [w, h] : { std::pair { 900, 640 }, std::pair { 760, 560 }, std::pair { 1400, 900 } })
    {
        const auto size = juce::String (w) + "x" + juce::String (h);
        panel.setSize (w, h);
        expectCleanLayout (panel, "panel " + size);
        for (auto* child : panel.getChildren())
            if (dynamic_cast<openmixer::ui::ChannelStripPanel*> (child) != nullptr
                || dynamic_cast<openmixer::ui::DeckStrip*> (child) != nullptr)
                expectCleanLayout (*child, "strip in " + size);
    }
}

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
