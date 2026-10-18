// The UI gate's harness: the app's own AppComposition (the same wiring MainComponent uses) in a plain host
// component, driven by a paced mock audio device and the gate's own control clock. Nothing else is composed
// here, so the gate cannot drift from the app. Message thread only.
#pragma once

#include "AppComposition.h"
#include "engine/MockAudioDevice.h"
#include "soak/OutputChecker.h"
#include "ui/GlassLookAndFeel.h"

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
    static constexpr double kRate = 48000.0;
    static constexpr int kBlock = 256;

    // Library errors reported by the services land here (must stay empty for the gate to pass).
    std::vector<juce::String> libraryErrors;
    std::vector<std::int64_t> trackIds;   // library ids of the generated tracks, in TrackSpec order

    UiRig (const juce::File& dir, const std::vector<TrackSpec>& tracks, int analysisWorkers)
        : composition (host, themes, dir.getChildFile ("preferences.json"), libraryOptions (dir, analysisWorkers))
    {
        host.setVisible (true);
        host.setSize (kWidth, kHeight);
        composition.setUpSettings ({ nullptr, [this] { return latencyStats(); },
                                     [] { return juce::Array<juce::MidiDeviceInfo>(); }, {}, {} });
        composition.layout();
        if (composition.hasLibrary())
            for (const auto& t : tracks)
                insert (t);
        if (auto* b = composition.getBrowser())
            b->refresh();
    }

    ~UiRig() { stop(); }

    bool isValid() const noexcept { return composition.hasLibrary(); }

    void start()
    {
        device.start (checker);
        device.startRealtime();
    }

    void stop()
    {
        device.stopRealtime();
        device.stop();
    }

    ui::MainView& getView() noexcept { return composition.getView(); }
    ui::MixerPanel& getMixer() noexcept { return composition.getMixer(); }
    ui::LibraryBrowser& getBrowser() noexcept { return *composition.getBrowser(); }
    ui::TrackLoader& getLoader() noexcept { return *composition.getLoader(); }
    juce::Component& getHost() noexcept { return host; }
    LatencyStats latencyStats() const { return device.getLatencyStats(); }
    OutputReport outputReport() const { return checker.getReport(); }

private:
    struct DefaultThemes : ui::ThemeRegistry
    {
        DefaultThemes()
        {
            loadBuiltIns();
            select (ui::ThemeRegistry::defaultId());
        }
    };

    // Resets the default look-and-feel after every component is gone.
    struct LookAndFeelScope
    {
        explicit LookAndFeelScope (ui::GlassLookAndFeel& l) { juce::LookAndFeel::setDefaultLookAndFeel (&l); }
        ~LookAndFeelScope() { juce::LookAndFeel::setDefaultLookAndFeel (nullptr); }
    };

    ui::LibraryServices::Options libraryOptions (const juce::File& dir, int analysisWorkers)
    {
        ui::LibraryServices::Options options;
        options.dbPath = dir.getChildFile ("gate.db").getFullPathName();
        options.watchFolders = false;   // tracks are inserted directly
        options.scanOnStart = false;
        options.analysisWorkers = analysisWorkers;
        options.onError = [this] (const juce::String& m) { libraryErrors.push_back (m); };
        return options;
    }

    void insert (const TrackSpec& t)
    {
        library::TrackRecord record;
        record.path = t.file.getFullPathName();
        record.title = t.title;
        record.artist = "OpenMixer Gate";
        record.durationSec = 40.0;
        if (const auto id = composition.getServices()->getLibrary().tracks().insert (record))
            trackIds.push_back (*id);
        else
            libraryErrors.push_back ("cannot insert " + t.title);
    }

    // Declaration order is start-up order; the composition goes before the device that renders its engine.
public:
    DefaultThemes themes;
private:
    ui::GlassLookAndFeel laf;
    LookAndFeelScope lafScope { laf };
    juce::Component host;
public:
    AppComposition composition;
    DjEngine& engine = composition.engine;
private:
    OutputChecker checker { composition.engine };
    MockAudioDevice device { kRate, kBlock, 0, 4 };
};

} // namespace openmixer::gate
