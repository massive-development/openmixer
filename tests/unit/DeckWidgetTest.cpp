// OpenMixer DeckWidgetTest - deck widgets route router ids, show engine state, and paint (needs GUI init from TestMain)
#include <JuceHeader.h>
#include "DeckTestUtil.h"
#include "controllers/ParameterRouter.h"
#include "ui/DeckWidget.h"
#include "ui/MainView.h"

#include <gtest/gtest.h>

#include <set>

using namespace openmixer;
using namespace openmixer::ui;

namespace
{
// Records every routed id and forwards to the real router, so tests check both the id and the engine effect.
struct RecordingTarget : ParameterTarget
{
    explicit RecordingTarget (ParameterTarget& next) : forward (next) {}
    bool setParameter (const juce::String& id, double value) override
    {
        calls.emplace_back (id, value);
        return forward.setParameter (id, value);
    }
    bool saw (const juce::String& id, double value) const
    {
        return std::any_of (calls.begin(), calls.end(), [&] (const auto& c) { return c.first == id && c.second == value; });
    }
    ParameterTarget& forward;
    std::vector<std::pair<juce::String, double>> calls;
};

struct Rig
{
    explicit Rig (int decks = 2) : engine (decks), recorder (router)
    {
        engine.prepare (48000.0, 256);
        router.registerTarget ("sync", &engine.getSync());
        for (int i = 0; i < decks; ++i)
            router.registerTarget ("deck." + juce::String (i), &engine.getDeckControl (i));
    }
    DeckBinding binding (int deck = 0) { return { engine, recorder, deck }; }

    DjEngine engine;
    ParameterRouter router;
    RecordingTarget recorder;
};

std::shared_ptr<const analysis::WaveformData> syntheticWaveform (int points = 512)
{
    analysis::WaveformLevel base;
    base.samplesPerPoint = 256;
    for (int i = 0; i < points; ++i)
    {
        const float env = 0.2F + 0.7F * std::abs (std::sin (static_cast<float> (i) * 0.05F));
        base.points.push_back ({ -env, env, env, env * 0.5F, env * (i % 2 == 0 ? 0.8F : 0.1F) });
    }
    return std::make_shared<analysis::WaveformData> (44100.0, static_cast<juce::int64> (points) * 256, std::move (base));
}

int distinctPixels (const juce::Image& image)
{
    std::set<juce::uint32> seen;
    for (int y = 0; y < image.getHeight(); ++y)
        for (int x = 0; x < image.getWidth(); ++x)
            seen.insert (image.getPixelAt (x, y).getARGB());
    return static_cast<int> (seen.size());
}

juce::Image render (juce::Component& c, int w, int h)
{
    c.setBounds (0, 0, w, h);
    return c.createComponentSnapshot (c.getLocalBounds());
}
}

TEST (DeckWidgetTest, TransportButtonsRouteTheirIdsAndMirrorEngineState)
{
    Rig rig;
    rig.engine.setGrid (0, std::make_shared<FixedTempoGrid> (120.0, 44100.0));
    TransportPanel panel (rig.binding());
    panel.setSize (600, 32);

    panel.button (TransportPanel::Play).onClick();
    EXPECT_TRUE (rig.recorder.saw ("deck.0.play", 1.0));
    EXPECT_TRUE (rig.engine.getDeck (0).isPlaying());
    EXPECT_EQ (panel.button (TransportPanel::Play).getButtonText(), "Pause");
    panel.button (TransportPanel::Play).onClick();
    EXPECT_TRUE (rig.recorder.saw ("deck.0.play", 0.0));

    panel.button (TransportPanel::Sync).onClick();
    EXPECT_TRUE (rig.recorder.saw ("deck.0.sync", 1.0));
    EXPECT_TRUE (rig.engine.getSync().isEnabled (0));
    EXPECT_TRUE (panel.button (TransportPanel::Sync).getToggleState());

    panel.button (TransportPanel::Master).onClick();
    EXPECT_TRUE (rig.recorder.saw ("deck.0.sync.master", 1.0));
    EXPECT_EQ (rig.engine.getSync().getLeader(), 0);
    panel.button (TransportPanel::Master).onClick();
    EXPECT_TRUE (rig.recorder.saw ("deck.0.sync.master", 0.0));
    EXPECT_EQ (rig.engine.getSync().getLeader(), -1);

    panel.button (TransportPanel::KeyLock).onClick();
    EXPECT_TRUE (rig.recorder.saw ("deck.0.keylock", 1.0));
    EXPECT_TRUE (rig.engine.getDeck (0).isKeylock());

    EXPECT_TRUE (rig.engine.getDeckControl (0).isQuantize()) << "quantize starts on";
    panel.button (TransportPanel::Quantize).onClick();
    EXPECT_TRUE (rig.recorder.saw ("deck.0.quantize", 0.0));
    EXPECT_FALSE (rig.engine.getDeckControl (0).isQuantize());

    panel.button (TransportPanel::Slip).onClick();
    EXPECT_TRUE (rig.recorder.saw ("deck.0.slip", 1.0));
    EXPECT_TRUE (rig.engine.getDeckControl (0).isSlipping());
}

TEST (DeckWidgetTest, CueStopsAndReturnsToStart)
{
    Rig rig;
    rig.engine.getDeck (0).play();
    TransportPanel panel (rig.binding());
    panel.button (TransportPanel::Cue).onClick();
    EXPECT_TRUE (rig.recorder.saw ("deck.0.play", 0.0));
    EXPECT_TRUE (rig.recorder.saw ("deck.0.seek", 0.0));
    EXPECT_FALSE (rig.engine.getDeck (0).isPlaying());
}

TEST (DeckWidgetTest, TransportOfSecondDeckUsesItsOwnPrefix)
{
    Rig rig;
    TransportPanel panel (rig.binding (1));
    panel.button (TransportPanel::KeyLock).onClick();
    EXPECT_TRUE (rig.recorder.saw ("deck.1.keylock", 1.0));
    EXPECT_TRUE (rig.engine.getDeck (1).isKeylock());
    EXPECT_FALSE (rig.engine.getDeck (0).isKeylock());
}

TEST (DeckWidgetTest, PitchFaderRoutesRateAndRangeLimitsIt)
{
    Rig rig;
    PitchFader pitch (rig.binding());
    pitch.setSize (84, 300);
    EXPECT_TRUE (rig.recorder.calls.empty()) << "building the widget must not touch the engine";

    pitch.getSlider().setValue (1.05, juce::sendNotificationSync);
    EXPECT_TRUE (rig.recorder.saw ("deck.0.rate", 1.05));
    EXPECT_DOUBLE_EQ (rig.engine.getDeck (0).getRate(), 1.05);

    pitch.getSlider().setValue (1.5, juce::sendNotificationSync);
    EXPECT_NEAR (pitch.getSlider().getValue(), 1.08, 1e-9) << "+-8% range clamps";

    pitch.setRangeIndex (2);
    EXPECT_DOUBLE_EQ (pitch.getRange(), 0.5);
    pitch.getSlider().setValue (1.4, juce::sendNotificationSync);
    EXPECT_NEAR (rig.engine.getDeck (0).getRate(), 1.4, 1e-9);

    pitch.setRangeIndex (0);
    EXPECT_NEAR (rig.engine.getDeck (0).getRate(), 1.08, 1e-9) << "narrowing the range pulls the tempo back";
}

TEST (DeckWidgetTest, PitchFaderFollowsRateChangesFromElsewhere)
{
    Rig rig;
    PitchFader pitch (rig.binding());
    rig.engine.getDeck (0).setRate (1.03);   // e.g. a sync nudge or MIDI
    pitch.refresh();
    EXPECT_NEAR (pitch.getSlider().getValue(), 1.03, 1e-9);
    EXPECT_EQ (rig.recorder.calls.size(), 0u) << "following the engine must not echo back";
}

TEST (DeckWidgetTest, HotcuePadsSetJumpAndClear)
{
    Rig rig;
    HotcuePads pads (rig.binding());
    pads.setSize (240, 72);
    EXPECT_FALSE (rig.engine.getDeck (0).getHotcue (2).has_value());

    pads.pad (2).onClick();
    EXPECT_TRUE (rig.recorder.saw ("deck.0.hotcue2", 1.0));
    EXPECT_TRUE (rig.engine.getDeck (0).getHotcue (2).has_value());
    EXPECT_TRUE (pads.pad (2).getToggleState()) << "set cue lights its pad";
    EXPECT_FALSE (pads.pad (3).getToggleState());

    pads.pressPad (2, true);
    EXPECT_TRUE (rig.recorder.saw ("deck.0.hotcue2.clear", 1.0));
    EXPECT_FALSE (rig.engine.getDeck (0).getHotcue (2).has_value());
    EXPECT_FALSE (pads.pad (2).getToggleState());

    const auto before = rig.recorder.calls.size();
    pads.pressPad (8, false);
    EXPECT_EQ (rig.recorder.calls.size(), before) << "out-of-range pad sends nothing";
}

TEST (DeckWidgetTest, LoopButtonsRouteExactIds)
{
    Rig rig;
    LoopControls loops (rig.binding());
    loops.setSize (400, 72);
    for (int i = 0; i < LoopControls::kNumSizes; ++i)
    {
        rig.recorder.calls.clear();
        loops.sizeButton (i).onClick();
        ASSERT_EQ (rig.recorder.calls.size(), 1u);
        EXPECT_EQ (rig.recorder.calls[0].first, "deck.0." + LoopControls::loopId (LoopControls::kSizes[static_cast<size_t> (i)]));
        EXPECT_EQ (rig.recorder.calls[0].second, 1.0);
    }
    EXPECT_EQ (LoopControls::loopId (0.25), "loop.quantized.0.25");

    rig.recorder.calls.clear();
    loops.exitButton().onClick();
    loops.reloopButton().onClick();
    EXPECT_TRUE (rig.recorder.saw ("deck.0.loop.exit", 1.0));
    EXPECT_TRUE (rig.recorder.saw ("deck.0.loop.reloop", 1.0));

    rig.recorder.calls.clear();
    loops.halve().onClick();
    EXPECT_TRUE (rig.recorder.calls.empty()) << "no loop to halve";
}

using DeckWidgetSlow = deck_test::DeckFixture;

TEST_F (DeckWidgetSlow, HalveAndDoubleScaleTheCurrentLoopAndExitReloopToggleIt)
{
    Rig rig;
    auto& deck = rig.engine.getDeck (0);
    ASSERT_TRUE (deck.load (wavFile));
    ASSERT_TRUE (deck_test::waitFor ([&] { return deck.getLengthFrames() > 0; }));
    rig.engine.setGrid (0, std::make_shared<FixedTempoGrid> (600.0, deck_test::kRate));   // 4410 frames per beat
    LoopControls loops (rig.binding());
    loops.setSize (400, 72);

    loops.sizeButton (3).onClick();   // 2 beats
    EXPECT_NEAR (loops.currentLengthBeats(), 2.0, 1e-6);
    EXPECT_TRUE (deck.isLooping());
    EXPECT_TRUE (loops.sizeButton (3).getToggleState());

    loops.twice().onClick();
    EXPECT_NEAR (loops.currentLengthBeats(), 4.0, 1e-6);
    loops.halve().onClick();
    loops.halve().onClick();
    EXPECT_NEAR (loops.currentLengthBeats(), 1.0, 1e-6);
    EXPECT_TRUE (rig.recorder.saw ("deck.0.loop.quantized.1.0", 1.0) || rig.recorder.saw ("deck.0.loop.quantized.1", 1.0));

    loops.exitButton().onClick();
    EXPECT_FALSE (deck.isLooping());
    EXPECT_FALSE (loops.reloopButton().getToggleState());
    loops.reloopButton().onClick();
    EXPECT_TRUE (deck.isLooping());
}

TEST_F (DeckWidgetSlow, QuantizeOffStartsTheLoopAtThePlayheadInsteadOfTheGrid)
{
    Rig rig;
    auto& deck = rig.engine.getDeck (0);
    ASSERT_TRUE (deck.load (wavFile));
    ASSERT_TRUE (deck_test::waitFor ([&] { return deck.getLengthFrames() > 0; }));
    rig.engine.setGrid (0, std::make_shared<FixedTempoGrid> (600.0, deck_test::kRate));
    deck.seek (6000);
    ASSERT_TRUE (deck_test::pumpUntilBuffered (deck, 64, 6000));

    rig.router.setParameter ("deck.0.loop.quantized.1", 1.0);
    EXPECT_EQ (deck.getLoop().start, 4410) << "snapped to the nearest beat";
    rig.router.setParameter ("deck.0.quantize", 0.0);
    rig.router.setParameter ("deck.0.loop.quantized.1", 1.0);
    EXPECT_EQ (deck.getLoop().start, 6000) << "exactly at the playhead";
}

TEST (DeckWidgetTest, OverviewClickMapsXToSeekFraction)
{
    Rig rig;
    OverviewWaveform overview (rig.binding());
    overview.setSize (400, 40);

    EXPECT_DOUBLE_EQ (overview.seekAt (100.0), 0.25);
    EXPECT_TRUE (rig.recorder.saw ("deck.0.seek", 0.25));
    EXPECT_DOUBLE_EQ (overview.seekAt (-30.0), 0.0);
    EXPECT_DOUBLE_EQ (overview.seekAt (999.0), 1.0);

    rig.recorder.calls.clear();
    const juce::MouseEvent down (juce::Desktop::getInstance().getMainMouseSource(), { 300.0F, 10.0F }, juce::ModifierKeys(),
                                 0.0F, 0.0F, 0.0F, 0.0F, 0.0F, &overview, &overview, juce::Time::getCurrentTime(), { 300.0F, 10.0F },
                                 juce::Time::getCurrentTime(), 1, false);
    overview.mouseDown (down);
    EXPECT_TRUE (rig.recorder.saw ("deck.0.seek", 0.75)) << "a real click path reaches the same mapping";
}

TEST_F (DeckWidgetSlow, ClickSeekMovesTheLoadedDeck)
{
    Rig rig;
    auto& deck = rig.engine.getDeck (0);
    ASSERT_TRUE (deck.load (wavFile));
    ASSERT_TRUE (deck_test::waitFor ([&] { return deck.getLengthFrames() > 0; }));
    OverviewWaveform overview (rig.binding());
    overview.setSize (200, 40);
    overview.seekAt (50.0);   // a quarter of the way in
    ASSERT_TRUE (deck_test::pumpUntilBuffered (deck, 64, deck.getLengthFrames() / 4));
    EXPECT_EQ (deck.getPositionFrames(), deck.getLengthFrames() / 4);
}

TEST (DeckWidgetTest, OverviewAndZoomPaintSyntheticWaveformInBandColours)
{
    Rig rig;
    OverviewWaveform overview (rig.binding());
    ZoomWaveform zoom (rig.binding());
    const auto emptyOverview = distinctPixels (render (overview, 400, 40));
    const auto emptyZoom = distinctPixels (render (zoom, 400, 120));

    const auto data = syntheticWaveform();
    overview.setWaveform (data);
    zoom.setWaveform (data);
    const auto overviewImage = render (overview, 400, 40);
    const auto zoomImage = render (zoom, 400, 120);
    ASSERT_FALSE (overviewImage.isNull());
    EXPECT_GT (distinctPixels (overviewImage), emptyOverview + 2) << "waveform adds colour";
    EXPECT_GT (distinctPixels (zoomImage), emptyZoom + 2);
}

TEST (DeckWidgetTest, ZoomDrawsGridLinesAndStaysInZoomRange)
{
    Rig rig;
    ZoomWaveform zoom (rig.binding());
    zoom.setWaveform (syntheticWaveform());
    const auto withoutGrid = distinctPixels (render (zoom, 400, 120));
    rig.engine.setGrid (0, std::make_shared<FixedTempoGrid> (120.0, 44100.0));
    zoom.setFramesPerPixel (200.0);
    const auto image = render (zoom, 400, 120);
    EXPECT_GT (distinctPixels (image), withoutGrid) << "grid lines add colours";

    zoom.setFramesPerPixel (1.0);
    EXPECT_DOUBLE_EQ (zoom.getFramesPerPixel(), wave::kMinFramesPerPixel);
    zoom.setFramesPerPixel (1.0e9);
    EXPECT_DOUBLE_EQ (zoom.getFramesPerPixel(), wave::kMaxFramesPerPixel);
    zoom.setFramesPerPixel (256.0);
    zoom.zoomBy (2.0);
    EXPECT_DOUBLE_EQ (zoom.getFramesPerPixel(), 512.0);
}

TEST (DeckWidgetTest, GeometryMapsPixelsAndFrames)
{
    EXPECT_DOUBLE_EQ (wave::xToFraction (50.0, 200.0), 0.25);
    EXPECT_DOUBLE_EQ (wave::xToFraction (5.0, 0.0), 0.0) << "zero width is safe";
    EXPECT_DOUBLE_EQ (wave::frameToX (500, 1000, 200.0), 100.0);
    EXPECT_DOUBLE_EQ (wave::frameToX (500, 0, 200.0), 0.0);

    const wave::View view { 10000, 100.0, 400.0 };
    EXPECT_DOUBLE_EQ (view.xOfFrame (10000), 200.0) << "the playhead frame sits at the centre";
    EXPECT_DOUBLE_EQ (view.xOfFrame (10100), 201.0);
    EXPECT_DOUBLE_EQ (view.frameAtX (0.0), -10000.0);
    EXPECT_DOUBLE_EQ (view.lastFrame(), 30000.0);
}

TEST (DeckWidgetTest, HeaderShowsTrackInfoClockAndEffectiveBpm)
{
    EXPECT_EQ (TrackHeader::formatTime (65.34, false), "1:05.3");
    EXPECT_EQ (TrackHeader::formatTime (12.0, true), "-0:12.0");
    EXPECT_EQ (TrackHeader::formatTime (-3.0, false), "0:00.0");
    EXPECT_EQ (TrackHeader::formatBpm (0.0), "--");
    EXPECT_EQ (TrackHeader::formatBpm (127.96), "128.0");
    EXPECT_EQ (TrackHeader::formatKey ({ "", "", "Am", "8A", 0.0 }), "Am  8A");
    EXPECT_EQ (TrackHeader::formatKey ({ "", "", "", "8A", 0.0 }), "8A");

    Rig rig;
    TrackHeader header (rig.binding());
    header.setSize (500, 52);
    header.setTrack ({ "Night Drive", "Some Artist", "Am", "8A", 124.0 });
    EXPECT_EQ (header.getTitleLabel().getText(), "Night Drive");
    EXPECT_EQ (header.getArtistLabel().getText(), "Some Artist");
    EXPECT_EQ (header.getKeyLabel().getText(), "Am  8A");
    EXPECT_EQ (header.getBpmLabel().getText(), "124.0");

    rig.engine.getDeck (0).setRate (1.05);
    header.refresh();
    EXPECT_EQ (header.getBpmLabel().getText(), "130.2") << "metadata bpm scaled by the pitch";

    rig.engine.setGrid (0, std::make_shared<FixedTempoGrid> (100.0, 44100.0));
    header.refresh();
    EXPECT_EQ (header.getBpmLabel().getText(), "105.0") << "the beat grid wins over metadata";
    EXPECT_EQ (header.getElapsedLabel().getText(), "0:00.0");
}

TEST (DeckWidgetTest, DeckWidgetComposesAndPaintsWithAndWithoutWaveform)
{
    Rig rig;
    DeckWidget widget (rig.binding());
    widget.setSize (560, 420);
    for (auto* child : widget.getChildren())
    {
        EXPECT_FALSE (child->getBounds().isEmpty());
        EXPECT_TRUE (widget.getLocalBounds().contains (child->getBounds()));
    }
    const auto empty = render (widget, 560, 420);
    ASSERT_FALSE (empty.isNull());

    widget.setWaveform (syntheticWaveform());
    widget.setGrid (std::make_shared<FixedTempoGrid> (120.0, 44100.0));
    EXPECT_NE (rig.engine.getDeckControl (0).getGrid(), nullptr) << "setGrid reaches the engine";
    widget.refresh();
    const auto full = render (widget, 560, 420);
    EXPECT_GT (distinctPixels (full), distinctPixels (empty));

    widget.setWaveform (nullptr);
    widget.refresh();
    EXPECT_FALSE (render (widget, 560, 420).isNull()) << "dropping the waveform returns to the empty state";
}

TEST (DeckWidgetTest, MainViewPlacesDecksAndPlaceholderSlots)
{
    Rig rig;
    MainView view (rig.engine, rig.recorder);
    view.setSize (1280, 800);
    ASSERT_EQ (view.getNumDecks(), 2);
    ASSERT_NE (view.getSlot (LayoutSpec::kMixerSlot), nullptr);
    ASSERT_NE (view.getSlot (LayoutSpec::kLibrarySlot), nullptr);

    const auto d0 = view.getDeck (0)->getBounds(), d1 = view.getDeck (1)->getBounds();
    const auto mixer = view.getSlot (LayoutSpec::kMixerSlot)->getBounds();
    const auto library = view.getSlot (LayoutSpec::kLibrarySlot)->getBounds();
    EXPECT_LT (d0.getRight(), mixer.getX());
    EXPECT_LT (mixer.getRight(), d1.getX()) << "the mixer sits between the decks";
    EXPECT_GT (library.getY(), d0.getBottom());
    for (const auto& r : { d0, d1, mixer, library })
        EXPECT_TRUE (view.getLocalBounds().contains (r));
    EXPECT_FALSE (d0.intersects (mixer) || d1.intersects (mixer) || mixer.intersects (library));
    EXPECT_FALSE (render (view, 1280, 800).isNull());
}

TEST (DeckWidgetTest, MainViewSlotsAcceptRealContentAndRejectUnknownNames)
{
    Rig rig;
    MainView view (rig.engine, rig.recorder);
    view.setSize (1280, 800);
    auto content = std::make_unique<juce::Label>();
    auto* raw = content.get();
    EXPECT_TRUE (view.setSlotContent (LayoutSpec::kMixerSlot, std::move (content)));
    EXPECT_EQ (view.getSlot (LayoutSpec::kMixerSlot), raw);
    EXPECT_EQ (raw->getBounds(), view.getSlot (LayoutSpec::kMixerSlot)->getBounds());
    EXPECT_FALSE (raw->getBounds().isEmpty());
    EXPECT_EQ (raw->getParentComponent(), &view);
    EXPECT_FALSE (view.setSlotContent ("nope", std::make_unique<juce::Label>()));
}

TEST (DeckWidgetTest, MainViewBuildsFourDecksFromTheStandardLayout)
{
    Rig rig (4);
    MainView view (rig.engine, rig.recorder);
    view.setSize (1600, 1000);
    ASSERT_EQ (view.getNumDecks(), 4);
    for (int i = 0; i < 4; ++i)
        EXPECT_FALSE (view.getDeck (i)->getBounds().isEmpty());
    view.getDeck (3)->getTransport().button (TransportPanel::KeyLock).onClick();
    EXPECT_TRUE (rig.recorder.saw ("deck.3.keylock", 1.0));
}

TEST (DeckWidgetTest, MainViewIgnoresLayoutDecksTheEngineLacks)
{
    Rig rig (2);
    MainView view (rig.engine, rig.recorder, LayoutSpec::standard (4));
    view.setSize (1600, 1000);
    EXPECT_EQ (view.getNumDecks(), 2);
    EXPECT_EQ (view.getDeck (3), nullptr);
}
