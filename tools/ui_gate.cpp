// Headless UI gate (Todo 31): the full main window (decks, mixer, FX, library browser, track loader)
// over the engine on a paced mock device, operated through the widgets by a scripted DJ (see
// ui_gate/UiScenario.h). Checks UI/engine agreement, xrun, underrun and output budgets like the soak.
// Usage: openmixer_ui_gate [--seconds N] [--max-xruns N] [--max-underruns N] [--analysis-timeout-ms N]
//                          [--tone-seconds N] [--workers N]; exit 0 = pass.
#include "engine/MockAudioDevice.h"
#include "soak/OutputChecker.h"
#include "soak/ToneWriter.h"
#include "ui_gate/UiRig.h"
#include "ui_gate/UiScenario.h"

#include <juce_events/juce_events.h>

#include <cstdio>
#include <cstdlib>
#include <cstring>

namespace
{

using namespace openmixer;

constexpr double kRate = 48000.0;
constexpr int kBlock = 256;
constexpr double kClickBpm = 120.0;

struct Options
{
    int seconds = 30;
    int maxXruns = 0;
    int maxUnderruns = 0;
    int analysisTimeoutMs = 15000;
    int toneSeconds = 40;
    int workers = 2;
};

Options parseArgs (int argc, char** argv)
{
    Options o;
    for (int i = 1; i + 1 < argc; i += 2)
    {
        const int value = std::atoi (argv[i + 1]);
        if (std::strcmp (argv[i], "--seconds") == 0)                   o.seconds = value;
        else if (std::strcmp (argv[i], "--max-xruns") == 0)            o.maxXruns = value;
        else if (std::strcmp (argv[i], "--max-underruns") == 0)        o.maxUnderruns = value;
        else if (std::strcmp (argv[i], "--analysis-timeout-ms") == 0)  o.analysisTimeoutMs = value;
        else if (std::strcmp (argv[i], "--tone-seconds") == 0)         o.toneSeconds = value;
        else if (std::strcmp (argv[i], "--workers") == 0)              o.workers = value;
    }
    return o;
}

bool makeTracks (const juce::File& dir, const Options& o, std::vector<gate::TrackSpec>& tracks)
{
    const char* titles[] = { "Aurora", "Borealis", "Cirrus" };
    for (int i = 0; i < 3; ++i)
    {
        const auto file = soak::writeTone (dir.getChildFile (juce::String (titles[i]) + ".wav"),
                                           { 220.0 * (i + 1), o.toneSeconds, kRate, kClickBpm });
        if (! file.existsAsFile())
            return false;
        tracks.push_back ({ titles[i], file });
    }
    return true;
}

bool report (const Options& o, const LatencyStats& stats, const gate::UiScenario& scenario, const OutputReport& out,
             const gate::UiRig& rig)
{
    const auto underruns = scenario.getSteadyUnderruns();
    const auto& c = scenario.getCounts();
    std::printf ("ui_gate: actions plays=%u syncs=%u crossfades=%u eq=%u hotcues=%u (clears %u) loops=%u (exits %u) fx=%u "
                 "searches=%u library-loads=%u themes=%u snapshots=%u keylocks=%u restarts=%u\n",
                 c.plays, c.syncs, c.crossfades, c.eqMoves, c.hotcues, c.hotcueClears, c.loops, c.loopExits, c.fxChanges,
                 c.searches, c.libraryLoads, c.themeSwitches, c.snapshots, c.keylocks, c.restarts);
    std::printf ("ui_gate: %llu callbacks, xruns=%llu, max callback %.0f us (budget %.0f us), avg cpu %.1f%%\n",
                 static_cast<unsigned long long> (stats.callbackCount), static_cast<unsigned long long> (stats.xrunCount),
                 stats.maxCallbackUs, 1.0e6 * kBlock / kRate, 100.0 * stats.cpuLoad);
    std::printf ("ui_gate: steady-state deck underruns=%u, non-finite=%llu, overflow=%llu, peak=%.3f, UI/engine mismatches=%u, "
                 "library errors=%zu\n", underruns, static_cast<unsigned long long> (out.nonFinite),
                 static_cast<unsigned long long> (out.overflow), static_cast<double> (out.peak), c.mismatches,
                 rig.libraryErrors.size());
    for (const auto& e : rig.libraryErrors)
        std::fprintf (stderr, "ui_gate: library error: %s\n", e.toRawUTF8());

    const bool covered = o.seconds < 20 || scenario.coversEveryArea();   // very short runs may miss a rare action
    if (! covered)
        std::fprintf (stderr, "ui_gate: some UI area was never exercised\n");
    return stats.callbackCount > 0 && stats.xrunCount <= static_cast<unsigned long long> (o.maxXruns)
        && underruns <= static_cast<unsigned> (o.maxUnderruns) && out.nonFinite == 0 && out.overflow == 0
        && out.peak > 0.01F && c.mismatches == 0 && rig.libraryErrors.empty() && covered;
}

// Everything is destroyed before the JUCE initialiser so the leak detector sees a clean exit.
int runGate (const Options& options)
{
    const auto dir = juce::File::getSpecialLocation (juce::File::tempDirectory).getNonexistentChildFile ("openmixer_ui_gate", {}, false);
    dir.createDirectory();
    std::vector<gate::TrackSpec> tracks;
    if (! makeTracks (dir, options, tracks))
    {
        std::fprintf (stderr, "ui_gate: cannot write the generated tracks\n");
        dir.deleteRecursively();
        return 2;
    }

    int result = 1;
    {
        gate::UiRig rig (dir, tracks, options.workers);
        if (! rig.isValid())
        {
            std::fprintf (stderr, "ui_gate: cannot build the main window rig\n");
            dir.deleteRecursively();
            return 2;
        }
        OutputChecker checker (rig.engine);
        MockAudioDevice device (kRate, kBlock, 0, 4);
        device.start (checker);
        device.startRealtime();

        gate::UiScenario scenario (rig, options.seconds);
        const auto start = juce::Time::getMillisecondCounter();
        const bool ready = scenario.begin (start, options.analysisTimeoutMs);
        const auto runStart = juce::Time::getMillisecondCounter();   // setup (analysis wait) is not part of the timed run
        for (auto t = runStart; ready && t - runStart < static_cast<juce::uint32> (options.seconds) * 1000U;
             t = juce::Time::getMillisecondCounter())
        {
            scenario.step (t, (t - runStart) / 1000.0);
            juce::MessageManager::getInstance()->runDispatchLoopUntil (gate::UiScenario::kControlPeriodMs);
        }

        device.stopRealtime();
        device.stop();
        result = ready && report (options, device.getLatencyStats(), scenario, checker.getReport(), rig) ? 0 : 1;
    }
    dir.deleteRecursively();
    return result;
}

} // namespace

int main (int argc, char** argv)
{
    juce::ScopedJuceInitialiser_GUI gui;
    return runGate (parseArgs (argc, argv));
}
