// Main window content: audio device glue, engine, MIDI, and the debug panel.
#pragma once

#include <JuceHeader.h>

#include "controllers/MidiInputRouter.h"
#include "controllers/MidiMapper.h"
#include "controllers/ParameterRouter.h"
#include "engine/DjEngine.h"
#include "ui/DebugPanel.h"

class MainComponent : public juce::AudioAppComponent, private juce::Timer
{
public:
    MainComponent();
    ~MainComponent() override;

    void prepareToPlay (int samplesPerBlockExpected, double sampleRate) override;
    void getNextAudioBlock (const juce::AudioSourceChannelInfo& bufferToFill) override;
    void releaseResources() override;

    void resized() override;

    void setBackdropOpacity (float opacity) { panel.setBackdropOpacity (opacity); }

private:
    void setUpMidi();
    void timerCallback() override;   // control tick: sync, FX tempo, slip

    openmixer::DjEngine engine;
    openmixer::ParameterRouter router;
    openmixer::MidiMapper mapper;
    openmixer::MidiInputRouter midiInput { deviceManager, mapper };
    openmixer::ui::DebugPanel panel { engine };
    double lastTickMs = 0.0;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR (MainComponent)
};
