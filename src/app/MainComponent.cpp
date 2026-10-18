#include "MainComponent.h"

namespace
{
    constexpr int kWindowWidth = 900, kWindowHeight = 640;
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
    addAndMakeVisible (panel);
    setSize (kWindowWidth, kWindowHeight);
    setUpMidi();
    setAudioChannels (0, 2);
    lastTickMs = juce::Time::getMillisecondCounterHiRes();
    startTimerHz (kControlTickHz);
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
    panel.setBounds (getLocalBounds());
}
