#include "AppComposition.h"

namespace openmixer
{

AppComposition::AppComposition (juce::Component& hostComponent, ui::ThemeRegistry& themeRegistry, const juce::File& prefs_,
                                std::optional<ui::LibraryServices::Options> libraryOptions)
    : host (hostComponent), themes (themeRegistry), prefsFile (prefs_)
{
    if (const auto r = ui::Preferences::load (prefsFile, prefs); r.failed())
        juce::Logger::writeToLog (r.getErrorMessage());
    setUpRouter();   // the router must hold its targets before the view is built
    buildContent();
    buildLibrary (std::move (libraryOptions));
    feedback.start();   // idle until a sink is set
}

AppComposition::~AppComposition()
{
    feedback.stop();
    trackLoader.reset();   // before the view it updates
}

void AppComposition::setUpRouter()
{
    router.registerTarget ("mixer", &engine.getMixer());
    router.registerTarget ("fx", &engine.getFx());
    router.registerTarget ("sync", &engine.getSync());
    for (int i = 0; i < engine.getNumDecks(); ++i)
        router.registerTarget ("deck." + juce::String (i), &engine.getDeckControl (i));   // decorates the Deck

    mapper.setTarget (&router);
    plugins.bindRouter (&router);   // plugin parameter targets appear under the router
}

// OPENMIXER_LAYOUT=<file> replaces the default arrangement with a LayoutSpec JSON file.
void AppComposition::buildContent()
{
    auto layoutSpec = ui::LayoutSpec::standard (engine.getNumDecks());
    const juce::File custom (juce::SystemStats::getEnvironmentVariable ("OPENMIXER_LAYOUT", {}));
    if (custom.existsAsFile())
    {
        if (const auto parsed = ui::LayoutSpec::fromJson (custom.loadFileAsString()))
            layoutSpec = *parsed;
        else
            juce::Logger::writeToLog ("OPENMIXER_LAYOUT: invalid layout, using the default");
    }
    mainView = std::make_unique<ui::MainView> (engine, router, layoutSpec);
    auto panel = std::make_unique<ui::MixerPanel> (ui::MixerBinding { engine, router });
    mixer = panel.get();
    mainView->setSlotContent (ui::LayoutSpec::kMixerSlot, std::move (panel));
    host.addAndMakeVisible (*mainView);
}

// OPENMIXER_LIBRARY_DB=<file> points the app at another library database (default: user data dir).
void AppComposition::buildLibrary (std::optional<ui::LibraryServices::Options> overrideOptions)
{
    juce::String error;
    auto options = overrideOptions.value_or (ui::LibraryServices::defaultOptions());
    if (! overrideOptions.has_value())
    {
        options.scanOnStart = prefs.library.scanOnStart;
        options.scanIntervalMs = static_cast<std::int64_t> (prefs.library.scanIntervalMinutes) * 60 * 1000;
    }
    libraryServices = ui::LibraryServices::create (std::move (options), &error);
    if (libraryServices == nullptr)
    {
        juce::Logger::writeToLog ("library unavailable: " + error);   // decks and mixer still work without it
        return;
    }
    trackLoader = std::make_unique<ui::TrackLoader> (engine, *mainView, *libraryServices);
    auto b = std::make_unique<ui::LibraryBrowser> (*libraryServices, engine.getNumDecks());
    browser = b.get();
    b->onLoadRequested = [this] (int deck, const library::TrackRecord& t) { return trackLoader->load (deck, t); };
    b->defaultDeck = [this] { return trackLoader->suggestedDeck(); };
    b->referenceKey = [this] { return trackLoader->referenceCamelot(); };
    mainView->setSlotContent (ui::LayoutSpec::kLibrarySlot, std::move (b));
    wireDeckDrops();
}

void AppComposition::wireDeckDrops()
{
    const juce::Component::SafePointer<ui::LibraryBrowser> shown (browser);
    for (int d = 0; d < mainView->getNumDecks(); ++d)
        mainView->getDeck (d)->onTrackDropped = [this, d, shown] (std::int64_t id)
        {
            if (shown != nullptr)
                if (const auto track = libraryServices->getLibrary().tracks().findById (id))
                    shown->requestLoad (d, *track);
        };
}

void AppComposition::setUpSettings (SettingsHooks h)
{
    hooks = std::move (h);
    settingsButton.onClick = [this] { openSettings(); };
    host.addAndMakeVisible (settingsButton);
    placeSettingsButton();
}

void AppComposition::openSettings()
{
    if (settingsPanel == nullptr)
    {
        ui::SettingsServices services { prefs, themes, hooks.audio, hooks.latency, hooks.midiInputs, libraryServices.get(),
                                        [this] { return plugins.getPluginIds(); }, hooks.reloadPlugins };
        settingsPanel = std::make_unique<ui::SettingsPanel> (std::move (services));
        settingsPanel->onChanged = [this] { preferencesChanged(); };
        settingsPanel->onClose = [this] { closeSettings(); };
        host.addChildComponent (*settingsPanel);
    }
    settingsPanel->reflectAll();
    settingsPanel->setBounds (host.getLocalBounds());
    settingsPanel->setVisible (true);
    settingsPanel->toFront (true);
}

void AppComposition::closeSettings()
{
    if (settingsPanel != nullptr)
        settingsPanel->setVisible (false);
    if (settingsButton.isShowing())   // focus needs a component that is on screen
        settingsButton.grabKeyboardFocus();
}

void AppComposition::preferencesChanged()
{
    if (const auto r = prefs.save (prefsFile); r.failed())
        juce::Logger::writeToLog ("preferences: " + r.getErrorMessage());
    if (hooks.preferencesChanged)
        hooks.preferencesChanged();
}

void AppComposition::layout()
{
    mainView->setBounds (host.getLocalBounds());
    placeSettingsButton();
    if (settingsPanel != nullptr)
        settingsPanel->setBounds (host.getLocalBounds());
}

// Sits in the free end of the mixer's tab row, so it never covers a control.
void AppComposition::placeSettingsButton()
{
    const auto* slot = mainView->getSlot (ui::LayoutSpec::kMixerSlot);
    if (slot == nullptr)
        return;
    auto row = slot->getBounds().withHeight (ui::theme::mixerTabHeight());
    settingsButton.setBounds (row.removeFromRight (ui::theme::mixerPageButtonWidth()));
}

} // namespace openmixer
