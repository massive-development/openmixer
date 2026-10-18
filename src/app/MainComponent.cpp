#include "MainComponent.h"

namespace
{
    constexpr int kWindowWidth = 1280, kWindowHeight = 800;
    constexpr int kMaxOutputs = 8;
    constexpr int kControlTickHz = 30;

    juce::File userMapFile()
    {
        return juce::File::getSpecialLocation (juce::File::userApplicationDataDirectory)
            .getChildFile ("OpenMixer").getChildFile ("controller_map.json");
    }
}

MainComponent::MainComponent()
{
    setUpMidi();   // the router must hold its targets before the view is built
    buildContent();
    setSize (kWindowWidth, kWindowHeight);
    setAudioChannels (0, 2);
    lastTickMs = juce::Time::getMillisecondCounterHiRes();
    startTimerHz (kControlTickHz);
}

// OPENMIXER_LAYOUT=<file> replaces the default arrangement with a LayoutSpec JSON file.
void MainComponent::buildContent()
{
    if (juce::SystemStats::getEnvironmentVariable ("OPENMIXER_DEBUG_PANEL", {}) == "1")
    {
        debugPanel = std::make_unique<openmixer::ui::DebugPanel> (engine);
        addAndMakeVisible (*debugPanel);
        return;
    }
    auto layout = openmixer::ui::LayoutSpec::standard (engine.getNumDecks());
    const juce::File custom (juce::SystemStats::getEnvironmentVariable ("OPENMIXER_LAYOUT", {}));
    if (custom.existsAsFile())
    {
        if (const auto parsed = openmixer::ui::LayoutSpec::fromJson (custom.loadFileAsString()))
            layout = *parsed;
        else
            juce::Logger::writeToLog ("OPENMIXER_LAYOUT: invalid layout, using the default");
    }
    mainView = std::make_unique<openmixer::ui::MainView> (engine, router, layout);
    addAndMakeVisible (*mainView);
}

void MainComponent::setBackdropOpacity (float opacity)
{
    if (mainView != nullptr)
        mainView->setBackdropOpacity (opacity);
    if (debugPanel != nullptr)
        debugPanel->setBackdropOpacity (opacity);
}

void MainComponent::timerCallback()
{
    const auto now = juce::Time::getMillisecondCounterHiRes();
    engine.updateControl ((now - lastTickMs) / 1000.0);
    lastTickMs = now;
}

MainComponent::~MainComponent()
{
    stopTimer();
    midiInput.disableAll();
    shutdownAudio();
}

void MainComponent::setUpMidi()
{
    router.registerTarget ("mixer", &engine.getMixer());
    router.registerTarget ("fx", &engine.getFx());
    router.registerTarget ("sync", &engine.getSync());
    for (int i = 0; i < engine.getNumDecks(); ++i)
        router.registerTarget ("deck." + juce::String (i), &engine.getDeckControl (i));   // decorates the Deck

    mapper.setTarget (&router);
    if (const auto map = userMapFile(); map.existsAsFile())
        if (const auto r = mapper.loadFromFile (map); r.failed())
            juce::Logger::writeToLog ("controller_map.json: " + r.getErrorMessage());
    midiInput.enableAll();
}

void MainComponent::prepareToPlay (int samplesPerBlockExpected, double sampleRate)
{
    engine.prepare (sampleRate, samplesPerBlockExpected);
}

void MainComponent::getNextAudioBlock (const juce::AudioSourceChannelInfo& info)
{
    auto* buffer = info.buffer;
    float* outputs[kMaxOutputs] {};
    const int numOutputs = std::min (buffer->getNumChannels(), kMaxOutputs);
    for (int c = 0; c < numOutputs; ++c)
        outputs[c] = buffer->getWritePointer (c, info.startSample);
    engine.process (nullptr, 0, outputs, numOutputs, info.numSamples);
}

void MainComponent::releaseResources() {}

void MainComponent::resized()
{
    if (mainView != nullptr)
        mainView->setBounds (getLocalBounds());
    if (debugPanel != nullptr)
        debugPanel->setBounds (getLocalBounds());
}
