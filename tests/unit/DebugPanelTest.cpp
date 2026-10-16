#include <JuceHeader.h>
#include "ui/ChannelStripPanel.h"
#include "ui/DebugPanel.h"

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
        }
    };

    DebugPanelTest debugPanelTest;
}
