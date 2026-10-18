// Main window content: audio device glue, engine, MIDI, and the main view (decks + mixer/library slots).
// The library slot is the browser; tracks load into decks through the TrackLoader (drag, double-click, Load buttons).
#pragma once

#include <JuceHeader.h>

#include "controllers/MidiInputRouter.h"
#include "controllers/MidiMapper.h"
#include "controllers/ParameterRouter.h"
#include "engine/DjEngine.h"
#include "ui/LibraryBrowser.h"
#include "ui/LibraryServices.h"
#include "ui/MainView.h"
#include "ui/MixerPanel.h"
#include "ui/TrackLoader.h"

class MainComponent : public juce::AudioAppComponent, public juce::DragAndDropContainer, private juce::Timer
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
    void buildLibrary();
    void timerCallback() override;   // control tick: sync, FX tempo, slip

    openmixer::DjEngine engine;
    openmixer::ParameterRouter router;
    openmixer::MidiMapper mapper;
    openmixer::MidiInputRouter midiInput { deviceManager, mapper };
    // Declaration order is start-up order (destroyed in reverse): services, view, then the loader that points at both.
    std::unique_ptr<openmixer::ui::LibraryServices> libraryServices;
    std::unique_ptr<openmixer::ui::MainView> mainView;
    std::unique_ptr<openmixer::ui::TrackLoader> trackLoader;
    double lastTickMs = 0.0;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR (MainComponent)
};
