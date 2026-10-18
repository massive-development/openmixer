// Scripted DJ for the UI gate: every action goes through the widgets' own interaction paths (button
// onClick, slider setValue with synchronous notification, table/browser handlers), then the gate checks
// that the widget and the engine agree. Message thread only.
#pragma once

#include "UiRig.h"
#include "soak/Scenario.h"
#include "ui/TrackDrag.h"

#include <cmath>
#include <cstdio>

namespace openmixer::gate
{

struct GateCounts
{
    unsigned plays = 0, syncs = 0, crossfades = 0, eqMoves = 0, hotcues = 0, hotcueClears = 0, loops = 0, loopExits = 0,
             fxChanges = 0, searches = 0, libraryLoads = 0, themeSwitches = 0, snapshots = 0, keylocks = 0, restarts = 0, drops = 0, settingsOpens = 0;
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
            && counts.themeSwitches > 1 && counts.snapshots > 0 && counts.drops > 0 && counts.settingsOpens > 0;
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

    // A headless host has no peer, so isShowing() is always false; check the same thing by hand: the widget and
    // every ancestor visible with non-empty bounds. Hidden or zero-size widgets count as mismatches.
    static bool shows (const juce::Component& c)
    {
        for (const auto* p = &c; p != nullptr; p = p->getParentComponent())
            if (! p->isVisible() || p->getWidth() <= 0 || p->getHeight() <= 0)
                return false;
        return true;
    }

    void need (const juce::Component& c, const char* what)
    {
        if (! shows (c))
        {
            std::fprintf (stderr, "ui_gate: widget not showing: %s\n", what);
            expect (false, "widget is showing with non-empty bounds before it is used");
        }
    }

    void press (juce::Button& b, const char* what)
    {
        need (b, what);
        b.onClick();
    }

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
        need (b.getSearchBox(), "search box");
        b.setSearchText (title);
        ++counts.searches;
        if (b.getModel().getNumRows() < 1)
        {
            std::fprintf (stderr, "ui_gate: search '%s' found no track\n", title);
            return false;
        }
        need (b.getTable(), "track table");
        b.getTable().selectRow (0);
        press (b.getLoadButton (d), "load button");
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
        press (transport (d).button (ui::TransportPanel::Play), "play button");
        ++counts.plays;
        expect (deck (d).isPlaying(), "play button starts the deck");
    }

    template <typename Read>
    void setAndCheck (juce::Slider& s, double v, Read engineValue, const char* what)
    {
        need (s, what);
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
        if (! dropped && fraction > 0.6)
            dropOnDeck (0, 1);
        if (! settingsDone && fraction > 0.9)
            exerciseSettings();
        if (switches == 0 && fraction > 0.5)
            switchTheme ("openmixer.daylight");
        if (switches == 1 && fraction > 0.75)
            switchTheme (ui::ThemeRegistry::defaultId());
        if (nowMs - lastSnapshotMs > kSnapshotEveryMs)
            snapshot (nowMs);
    }

    // Drag-and-drop path: the deck widget receives a library track description and loads it through the browser.
    void dropOnDeck (int d, size_t trackIndex)
    {
        dropped = true;
        need (deckWidget (d), "deck widget");
        const auto id = rig.trackIds[trackIndex];
        const juce::DragAndDropTarget::SourceDetails details (ui::trackdrag::describe (id), nullptr, {});
        expect (deckWidget (d).isInterestedInDragSource (details), "deck accepts a library track drag");
        deckWidget (d).itemDropped (details);
        ++counts.drops;
        underruns.noteDisturbance (now());
        const auto loaded = rig.getLoader().loadedTrack (d);
        expect (loaded.has_value() && loaded->id == id && deck (d).isLoaded(), "dropped track is loaded on the deck");
        ensureGrids (2000);
        startDeck (d);
    }

    // Gear button opens the overlay, every tab button selects its page, Close dismisses it.
    void exerciseSettings()
    {
        settingsDone = true;
        auto& composition = rig.composition;
        press (composition.getSettingsButton(), "settings button");
        auto* panel = composition.getSettingsPanel();
        expect (panel != nullptr && composition.isSettingsOpen(), "settings button opens the overlay");
        if (panel == nullptr)
            return;
        need (*panel, "settings panel");
        expect (panel->getBounds() == rig.getHost().getLocalBounds(), "settings overlay covers the window");
        for (int i = 0; i < panel->getNumPages(); ++i)
        {
            press (panel->getTabButton (i), "settings tab");
            expect (panel->getCurrentPage() == i, "tab button selects its page");
        }
        snapshot (now());
        press (panel->getCloseButton(), "settings close button");
        expect (! composition.isSettingsOpen(), "close button dismisses the overlay");
        ++counts.settingsOpens;
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
        const auto image = rig.getHost().createComponentSnapshot (rig.getHost().getLocalBounds(), true, 0.5F);
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
            press (loops.exitButton(), "loop exit button");
            ++counts.loopExits;
            expect (! deck (d).isLooping(), "exit button leaves the loop");
            return;
        }
        press (loops.sizeButton (rng.nextInt (5)), "loop size button");
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
            need (pads.pad (cue), "hotcue pad");
            pads.pressPad (cue, true);
            ++counts.hotcueClears;
            expect (! pads.pad (cue).getToggleState() && ! deck (d).getHotcue (cue).has_value(), "cleared hotcue pad goes dark");
            return;
        }
        const bool jump = deck (d).getHotcue (cue).has_value();
        press (pads.pad (cue), "hotcue pad");
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
        press (button, "key-lock button");
        ++counts.keylocks;
        underruns.noteDisturbance (nowMs);
        expect (button.getToggleState() == deck (1).isKeylock(), "key-lock button matches deck");
    }

    // FX tab (opened and closed through the page buttons): scope, unit, enable and mix, checked against the rack.
    void maybeFx()
    {
        if (rng.nextInt (30) != 0)
            return;
        auto& panel = rig.getMixer();
        press (panel.getPageButton (ui::MixerPanel::Page::Fx), "FX page button");
        editFx (panel.getFx());
        press (panel.getPageButton (ui::MixerPanel::Page::Mixer), "Mixer page button");   // strips are hidden on the FX page
    }

    void editFx (ui::FxPanel& fxPanel)
    {
        const int scope = rng.nextInt (fxPanel.getNumScopes());
        press (fxPanel.getScopeButton (scope), "FX scope button");
        auto& rack = scope == 0 ? rig.engine.getFx().getMasterRack() : rig.engine.getFx().getChannelRack (scope - 1);
        if (rack.getNumUnits() == 0)
            return;
        need (fxPanel.getUnitBox(), "FX unit box");
        fxPanel.getUnitBox().setSelectedItemIndex (rng.nextInt (rack.getNumUnits()), juce::sendNotificationSync);
        auto* unit = rack.getUnit (fxPanel.getUnit());
        if (unit == nullptr)
            return;
        const bool before = unit->isEnabled();
        press (fxPanel.getEnable(), "FX enable button");
        expect (unit->isEnabled() == ! before, "FX enable button toggles the unit");
        setAndCheck (fxPanel.getMix().getSlider(), 0.2 + 0.6 * rng.nextDouble(), [unit] { return unit->getMix(); }, "FX mix knob matches unit");
        ++counts.fxChanges;
    }

    // Query syntax in the search box; the table must show the rows the library holds.
    void maybeSearch()
    {
        if (rng.nextInt (80) != 0)
            return;
        static const std::pair<const char*, int> queries[] = { { "", 3 }, { "gate", 3 }, { "artist:openmixer", 3 }, { "borealis", 1 }, { "nomatchxyz", 0 } };
        const auto& q = queries[rng.nextInt (5)];
        need (rig.getBrowser().getSearchBox(), "search box");
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
        press (transport (d).button (ui::TransportPanel::Cue), "cue button");
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
                press (transport (d).button (ui::TransportPanel::Cue), "cue button");
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
    bool loadedC = false, dropped = false, settingsDone = false;
    int switches = 0;
    juce::uint32 lastSnapshotMs = 0;
};

} // namespace openmixer::gate
