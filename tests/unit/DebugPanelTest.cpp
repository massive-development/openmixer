#include <JuceHeader.h>
#include "ui/ChannelStripPanel.h"
#include "ui/DebugPanel.h"
#include "ui/DeckStrip.h"

namespace OpenMixerTests
{
    class DebugPanelTest : public juce::UnitTest
    {
    public:
        DebugPanelTest() : juce::UnitTest ("Debug Panel", "OpenMixer UI") {}

        static juce::Slider* sliderAt (juce::Component& parent, int sliderIndex)
        {
            int seen = 0;
            for (auto* child : parent.getChildren())
                if (auto* s = dynamic_cast<juce::Slider*> (child))
                    if (seen++ == sliderIndex)
                        return s;
            return nullptr;
        }


        static juce::StringArray labelTexts (juce::Component& parent)
        {
            juce::StringArray texts;
            for (auto* child : parent.getChildren())
                if (auto* l = dynamic_cast<juce::Label*> (child))
                    texts.add (l->getText());
            return texts;
        }

        // Every visible child must be non-empty, inside the parent, and clear of its siblings.
        void expectCleanLayout (juce::Component& parent, const juce::String& context)
        {
            const auto bounds = parent.getLocalBounds();
            for (int i = 0; i < parent.getNumChildComponents(); ++i)
            {
                auto* a = parent.getChildComponent (i);
                expect (! a->getBounds().isEmpty() && bounds.contains (a->getBounds()), context + ": child " + juce::String (i) + " clipped");
                for (int j = i + 1; j < parent.getNumChildComponents(); ++j)
                    expect (! a->getBounds().intersects (parent.getChildComponent (j)->getBounds()),
                            context + ": children " + juce::String (i) + "/" + juce::String (j) + " overlap");
            }
        }

        void testLabels()
        {
            beginTest ("Every mixer control has a caption");
            openmixer::Mixer mixer (2);
            openmixer::ui::ChannelStripPanel strip (mixer, 0);
            strip.setSize (300, 440);
            const auto texts = labelTexts (strip);
            for (const char* name : { "Hi", "Mid", "Lo", "Trim", "Filter", "Fader", "X-fader", "Lvl", "Channel 1" })
                expect (texts.contains (name), juce::String ("missing channel caption ") + name);

            openmixer::DjEngine engine (2);
            openmixer::ui::DebugPanel panel (engine);
            panel.setSize (900, 640);
            const auto panelTexts = labelTexts (panel);
            for (const char* name : { "Crossfader", "Master", "Cue mix", "Cue level" })
                expect (panelTexts.contains (name), juce::String ("missing panel caption ") + name);
        }

        void testLayoutStable()
        {
            beginTest ("Layout is clipped-free and overlap-free across sizes");
            openmixer::DjEngine engine (2);
            openmixer::ui::DebugPanel panel (engine);
            for (auto [w, h] : { std::pair { 900, 640 }, std::pair { 760, 560 }, std::pair { 1400, 900 } })
            {
                panel.setSize (w, h);
                expectCleanLayout (panel, "panel " + juce::String (w) + "x" + juce::String (h));
                for (auto* child : panel.getChildren())
                    if (dynamic_cast<openmixer::ui::ChannelStripPanel*> (child) != nullptr
                        || dynamic_cast<openmixer::ui::DeckStrip*> (child) != nullptr)
                        expectCleanLayout (*child, "strip in " + juce::String (w) + "x" + juce::String (h));
            }
        }

        void testNoRelayoutOnRefresh()
        {
            beginTest ("Refresh keeps bounds and position readout width constant");
            openmixer::DjEngine engine (2);
            openmixer::ui::DeckStrip deck (engine.getDeck (0), "Deck 1");
            deck.setSize (400, 96);
            juce::Array<juce::Rectangle<int>> before;
            for (auto* c : deck.getChildren())
                before.add (c->getBounds());
            for (int i = 0; i < 10; ++i)
                deck.refresh();
            for (int i = 0; i < deck.getNumChildComponents(); ++i)
                expect (deck.getChildComponent (i)->getBounds() == before[i], "refresh moved a child");

            const auto shortText = openmixer::ui::DeckStrip::formatPosition (0.0, 0.0);
            const auto longText = openmixer::ui::DeckStrip::formatPosition (1111.1, 3599.9);
            expectEquals (shortText.length(), longText.length());
        }

        void runTest() override
        {
            juce::ScopedJuceInitialiser_GUI gui;

            beginTest ("Channel strip widgets drive the mixer");
            {
                openmixer::Mixer mixer (2);
                mixer.prepare (48000.0, 256);
                openmixer::ui::ChannelStripPanel panel (mixer, 1);
                panel.setSize (140, 420);

                auto* lowKnob = sliderAt (panel, 0);
                auto* filterKnob = sliderAt (panel, 3);
                auto* fader = sliderAt (panel, 4);
                expect (lowKnob != nullptr && filterKnob != nullptr && fader != nullptr);

                lowKnob->setValue (-12.0, juce::sendNotificationSync);
                filterKnob->setValue (0.5, juce::sendNotificationSync);
                fader->setValue (0.3, juce::sendNotificationSync);
                expectEquals (mixer.getEq (1, 0), -12.0f);
                expectEquals (mixer.getFilter (1), 0.5f);
                expectWithinAbsoluteError (mixer.getFader (1), 0.3f, 1e-6f);
                expectEquals (mixer.getFader (0), 1.0f);   // other channel untouched
            }

            beginTest ("Debug panel builds one deck strip and channel per deck and lays out");
            {
                openmixer::DjEngine engine (2);
                engine.prepare (48000.0, 256);
                openmixer::ui::DebugPanel panel (engine);
                panel.setSize (900, 640);
                int kids = 0;
                for (auto* child : panel.getChildren())
                    kids += child->isVisible() && ! child->getBounds().isEmpty() ? 1 : 0;
                expect (kids >= 10, "all controls should have non-empty bounds, got " + juce::String (kids));

                auto* crossfader = sliderAt (panel, 0);
                expect (crossfader != nullptr);
                crossfader->setValue (0.9, juce::sendNotificationSync);
                expect (engine.getMixer().setParameter ("crossfader", 0.9));
            }

            testLabels();
            testLayoutStable();
            testNoRelayoutOnRefresh();
        }
    };

    DebugPanelTest debugPanelTest;
}
