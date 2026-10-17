// Headless soak: two looping decks through the mixer on a paced mock device while controls move.
// Usage: openmixer_soak [--seconds N] [--max-xruns N] [--max-underruns N]; exit 0 = pass.
#include "engine/DjEngine.h"
#include "engine/MockAudioDevice.h"
#include "soak/OutputChecker.h"

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
constexpr int kControlPeriodMs = 50;

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

juce::File writeTone (const juce::File& file, double hz)
{
    const int frames = static_cast<int> (kRate) * kToneSeconds;
    juce::AudioBuffer<float> buf (2, frames);
    juce::Random noise (7);
    for (int i = 0; i < frames; ++i)
    {
        const auto tone = 0.4F * std::sin (juce::MathConstants<float>::twoPi * static_cast<float> (hz * i / kRate));
        for (int c = 0; c < 2; ++c)
            buf.setSample (c, i, tone + 0.1F * (noise.nextFloat() - 0.5F));
    }

    file.deleteFile();
    juce::WavAudioFormat wav;
    std::unique_ptr<juce::FileOutputStream> stream (file.createOutputStream());
    if (stream == nullptr)
        return {};
    std::unique_ptr<juce::AudioFormatWriter> writer (wav.createWriterFor (stream.get(), kRate, 2, 24, {}, 0));
    if (writer == nullptr)
        return {};
    stream.release();   // the writer owns the stream
    writer->writeFromAudioSampleBuffer (buf, 0, frames);
    return file;
}

void wiggleControls (openmixer::DjEngine& engine, juce::Random& rng, double t)
{
    auto& mixer = engine.getMixer();
    mixer.setParameter ("crossfader", 0.5 + 0.5 * std::sin (t * 0.7));
    for (int ch = 0; ch < 2; ++ch)
    {
        const auto prefix = "ch" + juce::String (ch);
        mixer.setParameter (prefix + ".eq.low", -12.0 + 18.0 * rng.nextDouble());
        mixer.setParameter (prefix + ".eq.high", -12.0 + 18.0 * rng.nextDouble());
        mixer.setParameter (prefix + ".filter", 0.5 + 0.5 * std::sin (t * (0.3 + 0.2 * ch)));
        mixer.setParameter (prefix + ".fader", 0.6 + 0.4 * rng.nextDouble());
        engine.getDeck (ch).setParameter ("rate", 0.94 + 0.12 * rng.nextDouble());
    }
}

// Underruns right after a seek or restart are expected while the decoder refills; only steady-state ones count.
class UnderrunTracker
{
public:
    void noteDisturbance (juce::uint32 nowMs) noexcept { quietAt = nowMs + kSettleMs; }

    void tick (openmixer::DjEngine& engine, juce::uint32 nowMs) noexcept
    {
        unsigned total = 0;
        for (int d = 0; d < engine.getNumDecks(); ++d)
            total += engine.getDeck (d).getUnderrunCount();
        if (nowMs >= quietAt)
            steady += total - last;
        last = total;
    }

    unsigned getSteady() const noexcept { return steady; }

private:
    static constexpr juce::uint32 kSettleMs = 500;
    unsigned last = 0, steady = 0;
    juce::uint32 quietAt = 0;
};

void restartFinishedDecks (openmixer::DjEngine& engine, UnderrunTracker& tracker, juce::uint32 nowMs)
{
    for (int d = 0; d < engine.getNumDecks(); ++d)
        if (auto& deck = engine.getDeck (d); deck.hasFinished())
        {
            deck.seek (0);
            deck.play();
            tracker.noteDisturbance (nowMs);
        }
}

void occasionalSeek (openmixer::DjEngine& engine, juce::Random& rng, UnderrunTracker& tracker, juce::uint32 nowMs)
{
    if (rng.nextInt (40) != 0)
        return;
    engine.getDeck (rng.nextInt (engine.getNumDecks())).setParameter ("seek", rng.nextDouble());
    tracker.noteDisturbance (nowMs);
}

bool report (const Options& o, const openmixer::LatencyStats& stats, unsigned underruns,
             const openmixer::OutputReport& out)
{
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
    openmixer::OutputChecker checker (engine);
    openmixer::MockAudioDevice device (kRate, kBlock, 0, 4);

    for (int d = 0; d < engine.getNumDecks(); ++d)
    {
        const auto file = writeTone (dir.getChildFile ("tone" + juce::String (d) + ".wav"), 220.0 * (d + 1));
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

    juce::Random rng (1234);
    UnderrunTracker underruns;
    const auto start = juce::Time::getMillisecondCounter();
    underruns.noteDisturbance (start);
    for (auto now = start; now - start < static_cast<juce::uint32> (options.seconds) * 1000U;
         now = juce::Time::getMillisecondCounter())
    {
        wiggleControls (engine, rng, (now - start) / 1000.0);
        occasionalSeek (engine, rng, underruns, now);
        restartFinishedDecks (engine, underruns, now);
        underruns.tick (engine, now);
        juce::Thread::sleep (kControlPeriodMs);
    }

    device.stopRealtime();
    device.stop();
    dir.deleteRecursively();
    return report (options, device.getLatencyStats(), underruns.getSteady(), checker.getReport()) ? 0 : 1;
}
