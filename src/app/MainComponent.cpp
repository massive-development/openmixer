#include "MainComponent.h"

namespace
{
    constexpr int kWindowWidth = 1280, kWindowHeight = 800;
    constexpr int kMaxOutputs = openmixer::ui::JuceAudioSettingsModel::kMaxOutputs;
    constexpr int kControlTickHz = 30;
    constexpr double kMicrosPerSecond = 1.0e6;

    bool wantsSettingsOpen()
    {
        return juce::JUCEApplicationBase::getCommandLineParameters().contains ("--settings")
            || juce::SystemStats::getEnvironmentVariable ("OPENMIXER_SETTINGS", {}) == "1";
    }
}

// The composition loads the preferences first: they feed the library options, the saved audio state and
// (through the applier) plugins, theme, MIDI inputs and map. OPENMIXER_THEME=<id> still wins for the session.
MainComponent::MainComponent (openmixer::ui::ThemeRegistry& themeRegistry)
    : themes (themeRegistry), composition (*this, themeRegistry, openmixer::ui::Preferences::defaultFile())
{
    setSize (kWindowWidth, kWindowHeight);
    auto& prefs = composition.prefs;
    targets.noteAudioRequested (prefs.audio.deviceState);   // setAudioChannels below opens the device: not twice
    applier.apply (prefs);
    if (const auto wanted = juce::SystemStats::getEnvironmentVariable ("OPENMIXER_THEME", {}); wanted.isNotEmpty())
        targets.applyTheme (wanted);   // also the wanted theme, so a plugin reload keeps it
    const auto savedAudio = juce::parseXML (prefs.audio.deviceState);   // null: the system default device
    setAudioChannels (0, kMaxOutputs, savedAudio.get());
    composition.setUpSettings ({ &audioModel, [this] { return latencyStats(); },
                                 [] { return juce::MidiInput::getAvailableDevices(); },
                                 [this] { targets.reloadPlugins(); }, [this] { preferencesChanged(); } });
    composition.layout();
    lastTickMs = juce::Time::getMillisecondCounterHiRes();
    startTimerHz (kControlTickHz);
    if (wantsSettingsOpen())
        openSettings();
}

void MainComponent::setBackdropOpacity (float opacity)
{
    composition.getView().setBackdropOpacity (opacity);
}

void MainComponent::timerCallback()
{
    const auto now = juce::Time::getMillisecondCounterHiRes();
    composition.engine.updateControl ((now - lastTickMs) / 1000.0);
    lastTickMs = now;
}

MainComponent::~MainComponent()
{
    stopTimer();
    midiInput.disableAll();
    shutdownAudio();
}

// audioModel/deviceManager are base-class/earlier members: valid when `targets` is constructed.
openmixer::AudioDeviceControl MainComponent::audioControl()
{
    return { [this] { return audioModel.getStateXml(); },
             [this] (const juce::XmlElement& xml) { return deviceManager.initialise (0, kMaxOutputs, &xml, true); } };
}

void MainComponent::preferencesChanged()
{
    applier.apply (composition.prefs);
}

void MainComponent::prepareToPlay (int samplesPerBlockExpected, double sampleRate)
{
    composition.engine.prepare (sampleRate, samplesPerBlockExpected);
    outputs.prepare (samplesPerBlockExpected);
    latency.reset();
    latency.configure (sampleRate, samplesPerBlockExpected);
    if (auto* device = deviceManager.getCurrentAudioDevice())
        latency.setDeviceLatency (device->getInputLatencyInSamples(), device->getOutputLatencyInSamples());
}

void MainComponent::getNextAudioBlock (const juce::AudioSourceChannelInfo& info)
{
    auto* buffer = info.buffer;
    float* outputPtrs[kMaxOutputs] {};
    const int numOutputs = std::min (buffer->getNumChannels(), kMaxOutputs);
    for (int c = 0; c < numOutputs; ++c)
        outputPtrs[c] = buffer->getWritePointer (c, info.startSample);
    const auto start = juce::Time::getHighResolutionTicks();
    outputs.process (composition.engine, outputPtrs, numOutputs, info.numSamples);
    latency.recordCallback (juce::Time::highResolutionTicksToSeconds (juce::Time::getHighResolutionTicks() - start) * kMicrosPerSecond,
                            info.numSamples);
}

void MainComponent::releaseResources() {}

void MainComponent::resized()
{
    composition.layout();
}
