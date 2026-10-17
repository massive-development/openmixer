// Scripted DJ for the soak: moves mixer controls and rates, and drives loops, hotcues, key-lock, FX and seeks.
// Control thread only. Gapless actions (loop wraps/exits, key-lock on) are not excused from the underrun budget.
#pragma once

#include "engine/DjEngine.h"
#include "engine/fx/FxRack.h"

#include <cmath>

namespace openmixer::soak
{

struct ActionCounts
{
    unsigned loops = 0, loopExits = 0, hotcueJumps = 0, keylockToggles = 0, fxChanges = 0, seeks = 0;
};

// Underruns right after a seek or restart are expected while the decoder refills; only steady-state ones count.
class UnderrunTracker
{
public:
    void noteDisturbance (juce::uint32 nowMs) noexcept { quietAt = nowMs + kSettleMs; }

    void tick (DjEngine& engine, juce::uint32 nowMs) noexcept
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

class Scenario
{
public:
    static constexpr int kControlPeriodMs = 50;

    Scenario (DjEngine& e, fx::FxRack& masterFx) : engine (e), rack (masterFx) {}

    void begin (juce::uint32 nowMs)
    {
        underruns.noteDisturbance (nowMs);
        engine.getDeck (0).setKeylock (true);
        rack.setTempo (120.0);
    }

    void step (juce::uint32 nowMs, double seconds)
    {
        wiggleControls (seconds);
        maybeLoop();
        maybeHotcue (nowMs);
        maybeToggleKeylock (nowMs);
        maybeChangeFx();
        maybeSeek (nowMs);
        restartFinishedDecks (nowMs);
        underruns.tick (engine, nowMs);
    }

    unsigned getSteadyUnderruns() const noexcept { return underruns.getSteady(); }
    const ActionCounts& getCounts() const noexcept { return counts; }

private:
    static constexpr double kFramesPerBeat = 48000.0 * 60.0 / 120.0;   // 120 BPM at the soak's 48 kHz

    Deck& randomDeck() { return engine.getDeck (rng.nextInt (engine.getNumDecks())); }

    void wiggleControls (double t)
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

    // Loops from 1/4 to 4 beats at the playhead; held a few seconds, then exited. Both are gapless.
    void maybeLoop()
    {
        if (rng.nextInt (60) != 0)
            return;
        auto& deck = randomDeck();
        if (deck.isLooping())
        {
            deck.exitLoop();
            ++counts.loopExits;
            return;
        }
        const auto start = deck.getPositionFrames() + 4800;   // 100 ms ahead leaves time to prefetch
        const auto beats = 0.25 * static_cast<double> (1 << rng.nextInt (5));
        if (deck.setLoop ({ start, start + static_cast<std::int64_t> (beats * kFramesPerBeat) }))
            ++counts.loops;
    }

    void maybeHotcue (juce::uint32 nowMs)
    {
        if (rng.nextInt (80) != 0)
            return;
        auto& deck = randomDeck();
        const int cue = rng.nextInt (4);
        if (deck.getHotcue (cue).has_value())
        {
            ++counts.hotcueJumps;
            underruns.noteDisturbance (nowMs);   // a jump is a seek: the ring refills
        }
        deck.triggerHotcue (cue);
    }

    // Deck 1 flips key-lock; switching off drops stretcher input, which can starve briefly.
    void maybeToggleKeylock (juce::uint32 nowMs)
    {
        if (rng.nextInt (200) != 0)
            return;
        auto& deck = engine.getDeck (1);
        deck.setKeylock (! deck.isKeylock());
        ++counts.keylockToggles;
        underruns.noteDisturbance (nowMs);
    }

    // Flips one master FX unit and sets a random wet mix; tails ring out on disable.
    void maybeChangeFx()
    {
        if (rng.nextInt (30) != 0 || rack.getNumUnits() == 0)
            return;
        const auto unit = "fx" + juce::String (rng.nextInt (rack.getNumUnits()));
        rack.setParameter (unit + ".enable", rng.nextBool() ? 1.0 : 0.0);
        rack.setParameter (unit + ".mix", 0.2 + 0.6 * rng.nextDouble());
        ++counts.fxChanges;
    }

    void maybeSeek (juce::uint32 nowMs)
    {
        if (rng.nextInt (40) != 0)
            return;
        randomDeck().setParameter ("seek", rng.nextDouble());
        ++counts.seeks;
        underruns.noteDisturbance (nowMs);
    }

    void restartFinishedDecks (juce::uint32 nowMs)
    {
        for (int d = 0; d < engine.getNumDecks(); ++d)
            if (auto& deck = engine.getDeck (d); deck.hasFinished())
            {
                deck.seek (0);
                deck.play();
                underruns.noteDisturbance (nowMs);
            }
    }

    DjEngine& engine;
    fx::FxRack& rack;
    juce::Random rng { 1234 };
    UnderrunTracker underruns;
    ActionCounts counts;
};

} // namespace openmixer::soak
