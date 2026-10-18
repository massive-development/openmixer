// Scripted DJ for the UI gate: every action goes through the widgets' own interaction paths (button
// onClick, slider setValue with synchronous notification, table/browser handlers), then the gate checks
// that the widget and the engine agree. Message thread only.
#pragma once

#include "UiRig.h"
#include "soak/Scenario.h"

#include <cmath>
#include <cstdio>

namespace openmixer::gate
{

struct GateCounts
{
    unsigned plays = 0, syncs = 0, crossfades = 0, eqMoves = 0, hotcues = 0, hotcueClears = 0, loops = 0, loopExits = 0,
             fxChanges = 0, searches = 0, libraryLoads = 0, themeSwitches = 0, snapshots = 0, keylocks = 0, restarts = 0;
    unsigned mismatches = 0;
};

class UiScenario
{
public:
    static constexpr int kControlPeriodMs = 50;

    UiScenario (UiRig& r, double totalSeconds) : rig (r), total (totalSeconds) {}

    // Loads the first two tracks through the library browser and starts both decks from their Play buttons.
    bool begin (juce::uint32 nowMs, int analysisTimeoutMs)
    {
        underruns.noteDisturbance (nowMs);
        rig.getMixer().refresh (0.0F);
        for (int d = 0; d < 2; ++d)
            if (! loadFromBrowser (kTracks[d], d))
                return false;
        ensureGrids (analysisTimeoutMs);
        raiseFaders();
        startDeck (0);
        startDeck (1);
        press (rig.getView().getDeck (1)->getTransport().button (ui::TransportPanel::Sync), "sync on");
        press (rig.getView().getDeck (0)->getTransport().button (ui::TransportPanel::Master), "master");
        ++counts.syncs;
        frame (0.0F);
        expect (rig.engine.getSync().isEnabled (1), "sync button enables sync on deck 1");
        expect (rig.getView().getDeck (1)->getTransport().button (ui::TransportPanel::Sync).getToggleState(), "sync button lit");
        return true;
    }

    void step (juce::uint32 nowMs, double seconds)
    {
        const float dt = static_cast<float> (kControlPeriodMs) / 1000.0F;
        rig.engine.updateControl (kControlPeriodMs / 1000.0);   // the app's control timer
        timeline (nowMs, seconds / total);
        wiggleMixer (seconds);
        maybeLoop();
        maybeHotcue (nowMs);
        maybeKeylock (nowMs);
        maybeFx();
        maybeSearch();
        maybeRestart (nowMs);
        restartFinished (nowMs);
        frame (dt);
        checkAgreement();
        underruns.tick (rig.engine, nowMs);
    }

    unsigned getSteadyUnderruns() const noexcept { return underruns.getSteady(); }
    const GateCounts& getCounts() const noexcept { return counts; }
    bool coversEveryArea() const noexcept
    {
        return counts.plays > 0 && counts.syncs > 0 && counts.crossfades > 0 && counts.eqMoves > 0 && counts.hotcues > 0
            && counts.loops > 0 && counts.fxChanges > 0 && counts.searches > 0 && counts.libraryLoads > 2
            && counts.themeSwitches > 1 && counts.snapshots > 0;
    }

private:
    static constexpr const char* kTracks[3] = { "aurora", "borealis", "cirrus" };
    static constexpr double kFramesPerBeat = 48000.0 * 60.0 / 120.0;
    static constexpr double kTolerance = 0.01;

    ui::DeckWidget& deckWidget (int d) { return *rig.getView().getDeck (d); }
    Deck& deck (int d) { return rig.engine.getDeck (d); }
    ui::TransportPanel& transport (int d) { return deckWidget (d).getTransport(); }
    juce::uint32 now() const { return juce::Time::getMillisecondCounter(); }

    void expect (bool ok, const char* what)
    {
        if (ok)
            return;
        if (++counts.mismatches <= 8)
            std::fprintf (stderr, "ui_gate: MISMATCH %s\n", what);
    }

    static void press (juce::Button& b, const char*) { b.onClick(); }

    static void pumpMessages (int ms) { juce::MessageManager::getInstance()->runDispatchLoopUntil (ms); }

    // One frame tick, as the VBlank attachments would deliver while showing.
    void frame (float dt)
    {
        for (int d = 0; d < rig.getView().getNumDecks(); ++d)
            deckWidget (d).refresh();
        rig.getMixer().refresh (dt);
    }

    // Library search + load button, the same path as typing in the box and clicking "Load A".
    bool loadFromBrowser (const char* title, int d)
    {
        auto& b = rig.getBrowser();
        b.setSearchText (title);
        ++counts.searches;
        if (b.getModel().getNumRows() < 1)
        {
            std::fprintf (stderr, "ui_gate: search '%s' found no track\n", title);
            return false;
        }
        b.getTable().selectRow (0);
        b.getLoadButton (d).onClick();
        ++counts.libraryLoads;
        underruns.noteDisturbance (now());
        const auto loaded = rig.getLoader().loadedTrack (d);
        const bool ok = loaded.has_value() && loaded->title.equalsIgnoreCase (title) && deck (d).isLoaded();
        expect (ok, "library load puts the track on the deck");
        expect (deckWidget (d).getHeader().getTitleLabel().getText().equalsIgnoreCase (title), "deck header shows the loaded title");
        return ok;
    }

    // Real analysis fills the grid; a deck still without one after the timeout gets a constant 120 BPM grid.
    void ensureGrids (int timeoutMs)
    {
        const auto deadline = juce::Time::getMillisecondCounter() + static_cast<juce::uint32> (timeoutMs);
        while (juce::Time::getMillisecondCounter() < deadline && ! (hasGrid (0) && hasGrid (1)))
            pumpMessages (kControlPeriodMs);
        for (int d = 0; d < 2; ++d)
            if (! hasGrid (d))
            {
                std::fprintf (stderr, "ui_gate: deck %d has no analysed grid after %d ms, using a fixed 120 BPM grid\n", d, timeoutMs);
                deckWidget (d).setGrid (std::make_shared<FixedTempoGrid> (120.0, 48000.0));
            }
    }

    bool hasGrid (int d) { return rig.engine.getDeckControl (d).getGrid() != nullptr; }

    void raiseFaders()
    {
        for (int ch = 0; ch < 2; ++ch)
            setAndCheck (rig.getMixer().getChannel (ch).getFader(), 0.8, [this, ch] { return rig.engine.getMixer().getFader (ch); }, "fader");
    }

    void startDeck (int d)
    {
        press (transport (d).button (ui::TransportPanel::Play), "play");
        ++counts.plays;
        expect (deck (d).isPlaying(), "play button starts the deck");
    }

    template <typename Read>
    void setAndCheck (juce::Slider& s, double v, Read engineValue, const char* what)
    {
        s.setValue (v, juce::sendNotificationSync);
        expect (std::abs (static_cast<double> (engineValue()) - s.getValue()) < kTolerance, what);
    }

    // Theme switches and a second library load happen at fixed points of the run.
    void timeline (juce::uint32 nowMs, double fraction)
    {
        if (! loadedC && fraction > 0.25)
        {
            loadedC = true;
            loadFromBrowser (kTracks[2], 1);
            ensureGrids (2000);
            startDeck (1);
            if (! rig.engine.getSync().isEnabled (1))
                press (transport (1).button (ui::TransportPanel::Sync), "sync");
            ++counts.syncs;
        }
        if (switches == 0 && fraction > 0.5)
            switchTheme ("openmixer.daylight");
        if (switches == 1 && fraction > 0.75)
            switchTheme (ui::ThemeRegistry::defaultId());
        if (nowMs - lastSnapshotMs > kSnapshotEveryMs)
            snapshot (nowMs);
    }

    void switchTheme (const char* id)
    {
        ++switches;
        ++counts.themeSwitches;
        expect (rig.themes.select (id) && rig.themes.getActiveId() == id, "theme switch selects the theme");
        snapshot (now());
    }

    void snapshot (juce::uint32 nowMs)
    {
        lastSnapshotMs = nowMs;
        const auto image = rig.getView().createComponentSnapshot (rig.getView().getLocalBounds(), true, 0.5F);
        ++counts.snapshots;
        expect (image.isValid() && image.getWidth() > 0, "main view paints a non-empty snapshot");
    }

    void wiggleMixer (double t)
    {
        auto& panel = rig.getMixer();
        setAndCheck (panel.getCrossfader().getSlider(), 0.5 + 0.5 * std::sin (t * 0.7),
                     [this] { return rig.engine.getMixer().getCrossfader(); }, "crossfader slider matches mixer");
        ++counts.crossfades;
        for (int ch = 0; ch < 2; ++ch)
        {
            auto& strip = panel.getChannel (ch);
            setAndCheck (strip.getEq (0).getSlider(), -12.0 + 18.0 * rng.nextDouble(),
                         [this, ch] { return rig.engine.getMixer().getEq (ch, 0); }, "low EQ knob matches mixer");
            setAndCheck (strip.getEq (2).getSlider(), -12.0 + 18.0 * rng.nextDouble(),
                         [this, ch] { return rig.engine.getMixer().getEq (ch, 2); }, "high EQ knob matches mixer");
            setAndCheck (strip.getFilter().getSlider(), 0.5 + 0.5 * std::sin (t * (0.3 + 0.2 * ch)),
                         [this, ch] { return rig.engine.getMixer().getFilter (ch); }, "filter knob matches mixer");
            setAndCheck (strip.getFader(), 0.6 + 0.4 * rng.nextDouble(),
                         [this, ch] { return rig.engine.getMixer().getFader (ch); }, "channel fader matches mixer");
            counts.eqMoves += 2;
        }
        setAndCheck (deckWidget (0).getPitch().getSlider(), 0.95 + 0.1 * rng.nextDouble(),
                     [this] { return deck (0).getRate(); }, "pitch fader matches deck rate");   // deck 1 follows sync
    }

    // Loops from the size buttons (quantized to the grid); a second press path exits via the Exit button.
    void maybeLoop()
    {
        if (rng.nextInt (60) != 0)
            return;
        const int d = rng.nextInt (2);
        auto& loops = deckWidget (d).getLoops();
        if (deck (d).isLooping())
        {
            loops.exitButton().onClick();
            ++counts.loopExits;
            expect (! deck (d).isLooping(), "exit button leaves the loop");
            return;
        }
        loops.sizeButton (rng.nextInt (5)).onClick();
        ++counts.loops;
        underruns.noteDisturbance (now());
        expect (deck (d).isLooping(), "loop size button starts a loop");
        expect (loops.reloopButton().getToggleState(), "reloop button lit while looping");
    }

    void maybeHotcue (juce::uint32 nowMs)
    {
        if (rng.nextInt (60) != 0)
            return;
        const int d = rng.nextInt (2), cue = rng.nextInt (4);
        auto& pads = deckWidget (d).getHotcues();
        if (rng.nextInt (4) == 0 && deck (d).getHotcue (cue).has_value())
        {
            pads.pressPad (cue, true);
            ++counts.hotcueClears;
            expect (! pads.pad (cue).getToggleState() && ! deck (d).getHotcue (cue).has_value(), "cleared hotcue pad goes dark");
            return;
        }
        const bool jump = deck (d).getHotcue (cue).has_value();
        pads.pad (cue).onClick();
        ++counts.hotcues;
        if (jump)
            underruns.noteDisturbance (nowMs);   // a jump is a seek: the ring refills
        expect (pads.pad (cue).getToggleState() && deck (d).getHotcue (cue).has_value(), "hotcue pad lit after set");
    }

    void maybeKeylock (juce::uint32 nowMs)
    {
        if (rng.nextInt (200) != 0)
            return;
        auto& button = transport (1).button (ui::TransportPanel::KeyLock);
        button.onClick();
        ++counts.keylocks;
        underruns.noteDisturbance (nowMs);
        expect (button.getToggleState() == deck (1).isKeylock(), "key-lock button matches deck");
    }

    // FX tab: scope, unit, enable and mix, then the engine's rack must show the same.
    void maybeFx()
    {
        if (rng.nextInt (30) != 0)
            return;
        auto& panel = rig.getMixer();
        panel.setPage (ui::MixerPanel::Page::Fx);
        auto& fxPanel = panel.getFx();
        const int scope = rng.nextInt (fxPanel.getNumScopes());
        fxPanel.getScopeButton (scope).onClick();
        auto& rack = scope == 0 ? rig.engine.getFx().getMasterRack() : rig.engine.getFx().getChannelRack (scope - 1);
        if (rack.getNumUnits() == 0)
            return;
        fxPanel.getUnitBox().setSelectedItemIndex (rng.nextInt (rack.getNumUnits()), juce::sendNotificationSync);
        auto* unit = rack.getUnit (fxPanel.getUnit());
        if (unit == nullptr)
            return;
        const bool before = unit->isEnabled();
        fxPanel.getEnable().onClick();
        expect (unit->isEnabled() == ! before, "FX enable button toggles the unit");
        setAndCheck (fxPanel.getMix().getSlider(), 0.2 + 0.6 * rng.nextDouble(), [unit] { return unit->getMix(); }, "FX mix knob matches unit");
        ++counts.fxChanges;
        if (rng.nextInt (3) == 0)
            panel.setPage (ui::MixerPanel::Page::Mixer);
    }

    // Query syntax in the search box; the table must show the rows the library holds.
    void maybeSearch()
    {
        if (rng.nextInt (80) != 0)
            return;
        static const std::pair<const char*, int> queries[] = { { "", 3 }, { "gate", 3 }, { "artist:openmixer", 3 }, { "borealis", 1 }, { "nomatchxyz", 0 } };
        const auto& q = queries[rng.nextInt (5)];
        rig.getBrowser().setSearchText (q.first);
        ++counts.searches;
        expect (rig.getBrowser().getModel().getNumRows() == q.second, "search box row count matches the library");
    }

    // Cue (stop + rewind) then Play, both buttons.
    void maybeRestart (juce::uint32 nowMs)
    {
        if (rng.nextInt (250) != 0)
            return;
        const int d = rng.nextInt (2);
        transport (d).button (ui::TransportPanel::Cue).onClick();
        expect (! deck (d).isPlaying(), "cue button stops the deck");
        startDeck (d);
        ++counts.restarts;
        underruns.noteDisturbance (nowMs);
    }

    void restartFinished (juce::uint32 nowMs)
    {
        for (int d = 0; d < 2; ++d)
            if (deck (d).hasFinished())
            {
                transport (d).button (ui::TransportPanel::Cue).onClick();
                startDeck (d);
                underruns.noteDisturbance (nowMs);
            }
    }

    // Widgets mirror the engine after a frame tick.
    void checkAgreement()
    {
        for (int d = 0; d < 2; ++d)
        {
            auto& t = transport (d);
            expect (t.button (ui::TransportPanel::Play).getButtonText() == (deck (d).isPlaying() ? "Pause" : "Play"), "play button text matches deck");
            expect (t.button (ui::TransportPanel::KeyLock).getToggleState() == deck (d).isKeylock(), "key-lock lit matches deck");
            expect (t.button (ui::TransportPanel::Sync).getToggleState() == rig.engine.getSync().isEnabled (d), "sync lit matches sync");
            expect (deckWidget (d).getLoops().reloopButton().getToggleState() == deck (d).isLooping(), "reloop lit matches loop state");
            for (int c = 0; c < 8; ++c)
                if (deckWidget (d).getHotcues().pad (c).getToggleState() != deck (d).getHotcue (c).has_value())
                    expect (false, "hotcue pad lit matches cue");
        }
    }

    static constexpr juce::uint32 kSnapshotEveryMs = 5000;

    UiRig& rig;
    double total;
    juce::Random rng { 4321 };
    soak::UnderrunTracker underruns;
    GateCounts counts;
    bool loadedC = false;
    int switches = 0;
    juce::uint32 lastSnapshotMs = 0;
};

} // namespace openmixer::gate
