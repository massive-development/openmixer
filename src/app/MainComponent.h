// Main window content: audio device glue, engine, MIDI, and the main view (decks + mixer/library slots).
// The old debug console stays available behind OPENMIXER_DEBUG_PANEL=1 until the mixer slot is filled.
#pragma once

#include <JuceHeader.h>

#include "controllers/MidiInputRouter.h"
#include "controllers/MidiMapper.h"
#include "controllers/ParameterRouter.h"
#include "engine/DjEngine.h"
#include "ui/DebugPanel.h"
#include "ui/MainView.h"

class MainComponent : public juce::AudioAppComponent, private juce::Timer
{
public:
    MainComponent();
    ~MainComponent() override;

    void prepareToPlay (int samplesPerBlockExpected, double sampleRate) override;
    void getNextAudioBlock (const juce::AudioSourceChannelInfo& bufferToFill) override;
    void releaseResources() override;

    void resized() override;

    void setBackdropOpacity (float opacity);

private:
    void setUpMidi();
    void buildContent();
    void timerCallback() override;   // control tick: sync, FX tempo, slip

    openmixer::DjEngine engine;
    openmixer::ParameterRouter router;
    openmixer::MidiMapper mapper;
    openmixer::MidiInputRouter midiInput { deviceManager, mapper };
    std::unique_ptr<openmixer::ui::MainView> mainView;
    std::unique_ptr<openmixer::ui::DebugPanel> debugPanel;   // only with OPENMIXER_DEBUG_PANEL=1
    double lastTickMs = 0.0;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR (MainComponent)
};
