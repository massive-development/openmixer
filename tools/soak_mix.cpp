// Headless soak: two decks through the mixer on a paced mock device while a scripted DJ moves controls,
// loops, hotcues and key-lock (see soak/Scenario.h).
// Usage: openmixer_soak [--seconds N] [--max-xruns N] [--max-underruns N]; exit 0 = pass.
#include "engine/DjEngine.h"
#include "engine/MockAudioDevice.h"
#include "soak/OutputChecker.h"
#include "soak/Scenario.h"
#include "soak/ToneWriter.h"

#include <juce_audio_formats/juce_audio_formats.h>

#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>

namespace
{

constexpr double kRate = 48000.0;
constexpr int kBlock = 256;
constexpr int kToneSeconds = 6;

struct Options
{
    int seconds = 30;
    int maxXruns = 0;
    int maxUnderruns = 0;
};

Options parseArgs (int argc, char** argv)
{
    Options o;
    for (int i = 1; i + 1 < argc; i += 2)
    {
        const int value = std::atoi (argv[i + 1]);
        if (std::strcmp (argv[i], "--seconds") == 0)            o.seconds = value;
        else if (std::strcmp (argv[i], "--max-xruns") == 0)     o.maxXruns = value;
        else if (std::strcmp (argv[i], "--max-underruns") == 0) o.maxUnderruns = value;
    }
    return o;
}

bool report (const Options& o, const openmixer::LatencyStats& stats, const openmixer::soak::Scenario& scenario,
             const openmixer::OutputReport& out)
{
    const auto underruns = scenario.getSteadyUnderruns();
    const auto& c = scenario.getCounts();
    std::printf ("soak: actions loops=%u loop-exits=%u hotcue-jumps=%u keylock-toggles=%u fx-changes=%u seeks=%u\n",
                 c.loops, c.loopExits, c.hotcueJumps, c.keylockToggles, c.fxChanges, c.seeks);
    std::printf ("soak: %llu callbacks, xruns=%llu, max callback %.0f us (budget %.0f us), avg cpu %.1f%%\n",
                 static_cast<unsigned long long> (stats.callbackCount), static_cast<unsigned long long> (stats.xrunCount),
                 stats.maxCallbackUs, 1.0e6 * kBlock / kRate, 100.0 * stats.cpuLoad);
    std::printf ("soak: steady-state deck underruns=%u, non-finite=%llu, overflow=%llu, peak=%.3f\n", underruns,
                 static_cast<unsigned long long> (out.nonFinite), static_cast<unsigned long long> (out.overflow),
                 static_cast<double> (out.peak));

    return stats.callbackCount > 0 && stats.xrunCount <= static_cast<unsigned long long> (o.maxXruns)
        && underruns <= static_cast<unsigned> (o.maxUnderruns) && out.nonFinite == 0 && out.overflow == 0;
}

} // namespace

int main (int argc, char** argv)
{
    const auto options = parseArgs (argc, argv);

    const auto dir = juce::File::getSpecialLocation (juce::File::tempDirectory).getChildFile ("openmixer_soak");
    dir.createDirectory();

    openmixer::DjEngine engine;
    auto& masterFx = engine.getFx().getMasterRack();   // engine outlives the device below
    openmixer::OutputChecker checker (engine);
    openmixer::MockAudioDevice device (kRate, kBlock, 0, 4);

    for (int d = 0; d < engine.getNumDecks(); ++d)
    {
        const auto file = openmixer::soak::writeTone (dir.getChildFile ("tone" + juce::String (d) + ".wav"),
                                                      { 220.0 * (d + 1), kToneSeconds, kRate });
        if (! file.existsAsFile() || ! engine.getDeck (d).load (file))
        {
            std::fprintf (stderr, "soak: cannot prepare deck %d\n", d);
            return 2;
        }
        engine.getDeck (d).play();
        engine.getMixer().setParameter ("ch" + juce::String (d) + ".fader", 0.8);
    }

    device.start (checker);
    device.startRealtime();

    openmixer::soak::Scenario scenario (engine, masterFx);
    const auto start = juce::Time::getMillisecondCounter();
    scenario.begin (start);
    for (auto now = start; now - start < static_cast<juce::uint32> (options.seconds) * 1000U;
         now = juce::Time::getMillisecondCounter())
    {
        scenario.step (now, (now - start) / 1000.0);
        juce::Thread::sleep (openmixer::soak::Scenario::kControlPeriodMs);
    }

    device.stopRealtime();
    device.stop();
    dir.deleteRecursively();
    return report (options, device.getLatencyStats(), scenario, checker.getReport()) ? 0 : 1;
}
