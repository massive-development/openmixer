// The app's main-window composition without the audio device: engine + router + MainView with the mixer
// and library slots, services over a file library, and the track loader. Mirrors MainComponent's
// buildContent()/buildLibrary()/setUpMidi() so the gate drives the same wiring the app has.
// Message thread only.
#pragma once

#include "controllers/ParameterRouter.h"
#include "engine/DjEngine.h"
#include "ui/GlassLookAndFeel.h"
#include "ui/LibraryBrowser.h"
#include "ui/LibraryServices.h"
#include "ui/MainView.h"
#include "ui/MixerPanel.h"
#include "ui/ThemeRegistry.h"
#include "ui/TrackLoader.h"

#include <memory>
#include <vector>

namespace openmixer::gate
{

struct TrackSpec
{
    juce::String title;
    juce::File file;
};

class UiRig
{
public:
    static constexpr int kWidth = 1280, kHeight = 800;   // the app's default window

    // Library errors reported by the services land here (must stay empty for the gate to pass).
    std::vector<juce::String> libraryErrors;

    UiRig (const juce::File& dir, const std::vector<TrackSpec>& tracks, int analysisWorkers)
    {
        themes.loadBuiltIns();
        themes.select (ui::ThemeRegistry::defaultId());
        registerTargets();
        buildView();
        buildLibrary (dir, tracks, analysisWorkers);
        view->setBounds (0, 0, kWidth, kHeight);
    }

    bool isValid() const noexcept { return services != nullptr && browser != nullptr; }

    DjEngine engine;
    ParameterRouter router;
    ui::ThemeRegistry themes;
    ui::MainView& getView() noexcept { return *view; }
    ui::MixerPanel& getMixer() noexcept { return *mixer; }
    ui::LibraryBrowser& getBrowser() noexcept { return *browser; }
    ui::TrackLoader& getLoader() noexcept { return *loader; }
    ui::LibraryServices& getServices() noexcept { return *services; }

private:
    // Resets the default look-and-feel after every component below is gone.
    struct LookAndFeelScope
    {
        explicit LookAndFeelScope (ui::GlassLookAndFeel& l) { juce::LookAndFeel::setDefaultLookAndFeel (&l); }
        ~LookAndFeelScope() { juce::LookAndFeel::setDefaultLookAndFeel (nullptr); }
    };

    void registerTargets()
    {
        router.registerTarget ("mixer", &engine.getMixer());
        router.registerTarget ("fx", &engine.getFx());
        router.registerTarget ("sync", &engine.getSync());
        for (int i = 0; i < engine.getNumDecks(); ++i)
            router.registerTarget ("deck." + juce::String (i), &engine.getDeckControl (i));
    }

    void buildView()
    {
        view = std::make_unique<ui::MainView> (engine, router, ui::LayoutSpec::standard (engine.getNumDecks()));
        auto panel = std::make_unique<ui::MixerPanel> (ui::MixerBinding { engine, router });
        mixer = panel.get();
        view->setSlotContent (ui::LayoutSpec::kMixerSlot, std::move (panel));
    }

    void buildLibrary (const juce::File& dir, const std::vector<TrackSpec>& tracks, int analysisWorkers)
    {
        ui::LibraryServices::Options options;
        options.dbPath = dir.getChildFile ("gate.db").getFullPathName();
        options.watchFolders = false;   // tracks are inserted directly
        options.scanOnStart = false;
        options.analysisWorkers = analysisWorkers;
        options.onError = [this] (const juce::String& m) { libraryErrors.push_back (m); };
        juce::String error;
        services = ui::LibraryServices::create (std::move (options), &error);
        if (services == nullptr)
        {
            libraryErrors.push_back (error);
            return;
        }
        for (const auto& t : tracks)
            insert (t);
        loader = std::make_unique<ui::TrackLoader> (engine, *view, *services);
        auto b = std::make_unique<ui::LibraryBrowser> (*services, engine.getNumDecks());
        browser = b.get();
        b->onLoadRequested = [this] (int deck, const library::TrackRecord& t) { return loader->load (deck, t); };
        b->defaultDeck = [this] { return loader->suggestedDeck(); };
        b->referenceKey = [this] { return loader->referenceCamelot(); };
        view->setSlotContent (ui::LayoutSpec::kLibrarySlot, std::move (b));
        browser->refresh();
    }

    void insert (const TrackSpec& t)
    {
        library::TrackRecord record;
        record.path = t.file.getFullPathName();
        record.title = t.title;
        record.artist = "OpenMixer Gate";
        record.durationSec = 40.0;
        if (! services->getLibrary().tracks().insert (record).has_value())
            libraryErrors.push_back ("cannot insert " + t.title);
    }

    ui::GlassLookAndFeel laf;
    LookAndFeelScope lafScope { laf };
    std::unique_ptr<ui::LibraryServices> services;
    std::unique_ptr<ui::MainView> view;
    std::unique_ptr<ui::TrackLoader> loader;   // destroyed before the view and services it points at
    ui::MixerPanel* mixer = nullptr;
    ui::LibraryBrowser* browser = nullptr;
};

} // namespace openmixer::gate
