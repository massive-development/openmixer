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
    libraryServices = openmixer::ui::LibraryServices::create (openmixer::ui::LibraryServices::defaultOptions(), &error);
    if (libraryServices == nullptr)
    {
        juce::Logger::writeToLog ("library unavailable: " + error);   // decks and mixer still work without it
        return;
    }
    trackLoader = std::make_unique<openmixer::ui::TrackLoader> (engine, *mainView, *libraryServices);
    const auto report = [this] (const juce::Result& r) { if (r.failed()) juce::Logger::writeToLog ("load: " + r.getErrorMessage()); };

    auto browser = std::make_unique<openmixer::ui::LibraryBrowser> (*libraryServices, engine.getNumDecks());
    browser->onLoadRequested = [this, report] (int deck, const openmixer::library::TrackRecord& t) { report (trackLoader->load (deck, t)); };
    browser->defaultDeck = [this] { return trackLoader->suggestedDeck(); };
    browser->referenceKey = [this] { return trackLoader->referenceCamelot(); };
    mainView->setSlotContent (openmixer::ui::LayoutSpec::kLibrarySlot, std::move (browser));
    for (int d = 0; d < mainView->getNumDecks(); ++d)
        mainView->getDeck (d)->onTrackDropped = [this, d, report] (std::int64_t id) { report (trackLoader->load (d, id)); };
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
}
