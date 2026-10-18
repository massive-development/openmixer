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

// Preferences load first: they feed the library options, the saved audio state and (through the
// applier) plugins, theme, MIDI inputs and map. OPENMIXER_THEME=<id> still wins for the session.
MainComponent::MainComponent (openmixer::ui::ThemeRegistry& themeRegistry)
    : themes (themeRegistry), prefsFile (openmixer::ui::Preferences::defaultFile())
{
    if (const auto r = openmixer::ui::Preferences::load (prefsFile, prefs); r.failed())
        juce::Logger::writeToLog (r.getErrorMessage());
    setUpMidi();   // the router must hold its targets before the view is built
    buildContent();
    setSize (kWindowWidth, kWindowHeight);
    targets.noteAudioRequested (prefs.audio.deviceState);   // setAudioChannels below opens the device: not twice
    applier.apply (prefs);
    if (const auto wanted = juce::SystemStats::getEnvironmentVariable ("OPENMIXER_THEME", {}); wanted.isNotEmpty())
        themes.select (wanted);
    const auto savedAudio = juce::parseXML (prefs.audio.deviceState);   // null: the system default device
    setAudioChannels (0, kMaxOutputs, savedAudio.get());
    buildSettings();
    lastTickMs = juce::Time::getMillisecondCounterHiRes();
    startTimerHz (kControlTickHz);
    if (wantsSettingsOpen())
        openSettings();
}

// OPENMIXER_LAYOUT=<file> replaces the default arrangement with a LayoutSpec JSON file.
void MainComponent::buildContent()
{
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
    mainView->setSlotContent (openmixer::ui::LayoutSpec::kMixerSlot,
                              std::make_unique<openmixer::ui::MixerPanel> (openmixer::ui::MixerBinding { engine, router }));
    addAndMakeVisible (*mainView);
    buildLibrary();
}

// OPENMIXER_LIBRARY_DB=<file> points the app at another library database (default: user data dir).
void MainComponent::buildLibrary()
{
    juce::String error;
    auto options = openmixer::ui::LibraryServices::defaultOptions();
    options.scanOnStart = prefs.library.scanOnStart;
    options.scanIntervalMs = static_cast<std::int64_t> (prefs.library.scanIntervalMinutes) * 60 * 1000;
    libraryServices = openmixer::ui::LibraryServices::create (std::move (options), &error);
    if (libraryServices == nullptr)
    {
        juce::Logger::writeToLog ("library unavailable: " + error);   // decks and mixer still work without it
        return;
    }
    trackLoader = std::make_unique<openmixer::ui::TrackLoader> (engine, *mainView, *libraryServices);
    auto browser = std::make_unique<openmixer::ui::LibraryBrowser> (*libraryServices, engine.getNumDecks());
    browser->onLoadRequested = [this] (int deck, const openmixer::library::TrackRecord& t) { return trackLoader->load (deck, t); };
    browser->defaultDeck = [this] { return trackLoader->suggestedDeck(); };
    browser->referenceKey = [this] { return trackLoader->referenceCamelot(); };
    const juce::Component::SafePointer<openmixer::ui::LibraryBrowser> shown (browser.get());
    mainView->setSlotContent (openmixer::ui::LayoutSpec::kLibrarySlot, std::move (browser));
    for (int d = 0; d < mainView->getNumDecks(); ++d)
        mainView->getDeck (d)->onTrackDropped = [this, d, shown] (std::int64_t id)
        {
            if (shown != nullptr)
                if (const auto track = libraryServices->getLibrary().tracks().findById (id))
                    shown->requestLoad (d, *track);
        };
}

void MainComponent::setBackdropOpacity (float opacity)
{
    if (mainView != nullptr)
        mainView->setBackdropOpacity (opacity);
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
    trackLoader.reset();   // before the view it updates
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
    plugins.bindRouter (&router);   // plugin parameter targets appear under the router
}

// audioModel/deviceManager are base-class/earlier members: valid when `targets` is constructed.
openmixer::AudioDeviceControl MainComponent::audioControl()
{
    return { [this] { return audioModel.getStateXml(); },
             [this] (const juce::XmlElement& xml) { return deviceManager.initialise (0, kMaxOutputs, &xml, true); } };
}

void MainComponent::buildSettings()
{
    settingsButton.onClick = [this] { openSettings(); };
    addAndMakeVisible (settingsButton);
}

void MainComponent::openSettings()
{
    if (settingsPanel == nullptr)
    {
        openmixer::ui::SettingsServices services { prefs, themes, &audioModel, [this] { return latencyStats(); },
                                                   [] { return juce::MidiInput::getAvailableDevices(); },
                                                   libraryServices.get(),
                                                   [this] { return plugins.getPluginIds(); },
                                                   [this] { targets.reloadPlugins(); } };
        settingsPanel = std::make_unique<openmixer::ui::SettingsPanel> (std::move (services));
        settingsPanel->onChanged = [this] { preferencesChanged(); };
        settingsPanel->onClose = [this] { closeSettings(); };
        addChildComponent (*settingsPanel);
    }
    settingsPanel->reflectAll();
    settingsPanel->setBounds (getLocalBounds());
    settingsPanel->setVisible (true);
    settingsPanel->toFront (true);
}

void MainComponent::closeSettings()
{
    if (settingsPanel != nullptr)
        settingsPanel->setVisible (false);
    settingsButton.grabKeyboardFocus();
}

void MainComponent::preferencesChanged()
{
    if (const auto r = prefs.save (prefsFile); r.failed())
        juce::Logger::writeToLog ("preferences: " + r.getErrorMessage());
    applier.apply (prefs);
}

void MainComponent::prepareToPlay (int samplesPerBlockExpected, double sampleRate)
{
    engine.prepare (sampleRate, samplesPerBlockExpected);
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
    outputs.process (engine, outputPtrs, numOutputs, info.numSamples);
    latency.recordCallback (juce::Time::highResolutionTicksToSeconds (juce::Time::getHighResolutionTicks() - start) * kMicrosPerSecond,
                            info.numSamples);
}

void MainComponent::releaseResources() {}

void MainComponent::resized()
{
    if (mainView != nullptr)
        mainView->setBounds (getLocalBounds());
    placeSettingsButton();
    if (settingsPanel != nullptr)
        settingsPanel->setBounds (getLocalBounds());
}

// Sits in the free end of the mixer's tab row, so it never covers a control.
void MainComponent::placeSettingsButton()
{
    const auto* mixer = mainView != nullptr ? mainView->getSlot (openmixer::ui::LayoutSpec::kMixerSlot) : nullptr;
    if (mixer == nullptr)
        return;
    auto row = mixer->getBounds().withHeight (openmixer::ui::theme::mixerTabHeight());
    settingsButton.setBounds (row.removeFromRight (openmixer::ui::theme::mixerPageButtonWidth()));
}
