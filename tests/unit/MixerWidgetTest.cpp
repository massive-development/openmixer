// OpenMixer MixerWidgetTest - mixer widgets route router ids, mirror engine state, meter and paint
// (needs the GUI init from TestMain)
#include <JuceHeader.h>
#include "MixerTestUtil.h"
#include "ui/DebugPanel.h"
#include "ui/GlassLookAndFeel.h"
#include "ui/MainView.h"
#include "ui/MixerPanel.h"
#include "ui/ThemeRegistry.h"

#include <gtest/gtest.h>

#include <set>

using namespace openmixer;
using namespace openmixer::ui;
using mixertest::Rig;

namespace
{
constexpr float kFrame = 1.0F / 30.0F;

juce::Image render (juce::Component& c, int w, int h)
{
    c.setBounds (0, 0, w, h);
    return c.createComponentSnapshot (c.getLocalBounds());
}

bool hasContent (const juce::Image& img)
{
    std::set<juce::uint32> seen;
    for (int y = 0; y < img.getHeight(); y += 3)
        for (int x = 0; x < img.getWidth(); x += 3)
            seen.insert (img.getPixelAt (x, y).getARGB());
    return seen.size() > 2;
}

void expectNoOverlap (const std::vector<juce::Component*>& parts, const juce::String& context)
{
    for (size_t i = 0; i < parts.size(); ++i)
        for (size_t j = i + 1; j < parts.size(); ++j)
            EXPECT_FALSE (parts[i]->getBounds().intersects (parts[j]->getBounds())) << context << ": " << i << "/" << j << " overlap";
}
}

TEST (MixerWidgetTest, ChannelKnobsAndFaderRouteMixerIds)
{
    Rig rig;
    ChannelStripWidget strip (rig.binding(), 1);
    strip.setSize (150, 440);

    strip.getTrim().getSlider().setValue (2.0, juce::sendNotificationSync);
    strip.getFilter().getSlider().setValue (-0.5, juce::sendNotificationSync);
    strip.getEq (0).getSlider().setValue (-6.0, juce::sendNotificationSync);
    strip.getEq (1).getSlider().setValue (3.0, juce::sendNotificationSync);
    strip.getEq (2).getSlider().setValue (-12.0, juce::sendNotificationSync);
    strip.getFader().setValue (0.3, juce::sendNotificationSync);

    EXPECT_TRUE (rig.recorder.saw ("mixer.ch1.trim", 2.0));
    EXPECT_TRUE (rig.recorder.saw ("mixer.ch1.filter", -0.5));
    EXPECT_TRUE (rig.recorder.saw ("mixer.ch1.eq.low", -6.0));
    EXPECT_TRUE (rig.recorder.saw ("mixer.ch1.eq.mid", 3.0));
    EXPECT_TRUE (rig.recorder.saw ("mixer.ch1.eq.high", -12.0));
    EXPECT_TRUE (rig.recorder.saw ("mixer.ch1.fader", 0.3));

    auto& m = rig.engine.getMixer();
    EXPECT_NEAR (m.getTrim (1), 2.0F, 1e-6F);
    EXPECT_NEAR (m.getFilter (1), -0.5F, 1e-6F);
    EXPECT_NEAR (m.getEq (1, 0), -6.0F, 1e-6F);
    EXPECT_NEAR (m.getEq (1, 1), 3.0F, 1e-6F);
    EXPECT_NEAR (m.getEq (1, 2), -12.0F, 1e-6F);
    EXPECT_NEAR (m.getFader (1), 0.3F, 1e-6F);
    EXPECT_NEAR (m.getFader (0), 1.0F, 1e-6F) << "other channel untouched";
}

TEST (MixerWidgetTest, PflAndAssignButtonsRouteAndMirrorTheEngine)
{
    Rig rig;
    ChannelStripWidget strip (rig.binding(), 0);
    strip.setSize (150, 440);

    strip.getPfl().onClick();
    EXPECT_TRUE (rig.recorder.saw ("mixer.ch0.pfl", 1.0));
    EXPECT_TRUE (rig.engine.getMixer().getPfl (0));
    EXPECT_TRUE (strip.getPfl().getToggleState());
    strip.getPfl().onClick();
    EXPECT_TRUE (rig.recorder.saw ("mixer.ch0.pfl", 0.0));
    EXPECT_FALSE (strip.getPfl().getToggleState());

    strip.getAssign (CrossfaderAssign::Left).onClick();
    EXPECT_TRUE (rig.recorder.saw ("mixer.ch0.xfader", 1.0));
    EXPECT_EQ (rig.engine.getMixer().getCrossfaderAssign (0), CrossfaderAssign::Left);
    EXPECT_TRUE (strip.getAssign (CrossfaderAssign::Left).getToggleState());
    strip.getAssign (CrossfaderAssign::Right).onClick();
    EXPECT_TRUE (rig.recorder.saw ("mixer.ch0.xfader", 2.0));
    EXPECT_FALSE (strip.getAssign (CrossfaderAssign::Left).getToggleState());
    strip.getAssign (CrossfaderAssign::Thru).onClick();
    EXPECT_EQ (rig.engine.getMixer().getCrossfaderAssign (0), CrossfaderAssign::Thru);
}

TEST (MixerWidgetTest, EqKillDropsTheBandAndReleaseRestoresIt)
{
    Rig rig;
    ChannelStripWidget strip (rig.binding(), 0);
    strip.setSize (150, 440);
    strip.getEq (1).getSlider().setValue (4.0, juce::sendNotificationSync);

    strip.getKill (1).onClick();
    EXPECT_TRUE (rig.recorder.saw ("mixer.ch0.eq.mid", Mixer::kMinEqDb));
    EXPECT_EQ (rig.engine.getMixer().getEq (0, 1), Mixer::kMinEqDb);
    EXPECT_TRUE (strip.getKill (1).getToggleState());
    EXPECT_FALSE (strip.getKill (0).getToggleState()) << "other bands unaffected";
    EXPECT_NEAR (strip.getEq (1).getValue(), Mixer::kMinEqDb, 1e-4);

    strip.getKill (1).onClick();
    EXPECT_NEAR (rig.engine.getMixer().getEq (0, 1), 4.0F, 1e-6F);
    EXPECT_FALSE (strip.getKill (1).getToggleState());
}

TEST (MixerWidgetTest, EveryBandKillRoutesItsOwnId)
{
    Rig rig;
    ChannelStripWidget strip (rig.binding(), 0);
    strip.setSize (150, 440);
    const char* ids[3] { "mixer.ch0.eq.low", "mixer.ch0.eq.mid", "mixer.ch0.eq.high" };
    for (int band = 0; band < 3; ++band)
    {
        strip.getKill (band).onClick();
        EXPECT_TRUE (rig.recorder.saw (ids[band], Mixer::kMinEqDb)) << ids[band];
    }
}

TEST (MixerWidgetTest, FxButtonRequestsTheChannelsRack)
{
    Rig rig;
    ChannelStripWidget strip (rig.binding(), 1);
    int requested = -1;
    strip.onFxRequested = [&] (int ch) { requested = ch; };
    strip.getFxButton().onClick();
    EXPECT_EQ (requested, 1);
}

TEST (MixerWidgetTest, CrossfaderSectionRoutesPositionAndCurve)
{
    Rig rig;
    CrossfaderSection section (rig.binding());
    section.setSize (400, 52);
    EXPECT_TRUE (section.getCurveButton (CrossfaderCurve::ConstantPower).getToggleState());

    section.getSlider().setValue (0.8, juce::sendNotificationSync);
    EXPECT_TRUE (rig.recorder.saw ("mixer.crossfader", 0.8));
    EXPECT_NEAR (rig.engine.getMixer().getCrossfader(), 0.8F, 1e-6F);

    section.getCurveButton (CrossfaderCurve::Linear).onClick();
    EXPECT_TRUE (rig.recorder.saw ("mixer.crossfader.curve", 0.0));
    EXPECT_EQ (rig.engine.getMixer().getCrossfaderCurve(), CrossfaderCurve::Linear);
    EXPECT_TRUE (section.getCurveButton (CrossfaderCurve::Linear).getToggleState());
    section.getCurveButton (CrossfaderCurve::ConstantPower).onClick();
    EXPECT_TRUE (rig.recorder.saw ("mixer.crossfader.curve", 1.0));
    EXPECT_EQ (rig.engine.getMixer().getCrossfaderCurve(), CrossfaderCurve::ConstantPower);
}

TEST (MixerWidgetTest, MasterAndCueRouteTheirIds)
{
    Rig rig;
    MasterSection master (rig.binding());
    master.setSize (112, 300);
    master.getFader().setValue (0.7, juce::sendNotificationSync);
    EXPECT_TRUE (rig.recorder.saw ("mixer.master", 0.7));
    EXPECT_NEAR (rig.engine.getMixer().getMasterGain(), 0.7F, 1e-6F);

    CueSection cue (rig.binding());
    cue.setSize (400, 52);
    cue.getMix().setValue (0.6, juce::sendNotificationSync);
    cue.getLevel().setValue (0.4, juce::sendNotificationSync);
    EXPECT_TRUE (rig.recorder.saw ("mixer.cue.mix", 0.6));
    EXPECT_TRUE (rig.recorder.saw ("mixer.cue.level", 0.4));
    EXPECT_NEAR (rig.engine.getMixer().getCueMix(), 0.6F, 1e-6F);
    EXPECT_NEAR (rig.engine.getMixer().getCueLevel(), 0.4F, 1e-6F);
}

TEST (MixerWidgetTest, RefreshFollowsChangesMadeElsewhere)
{
    Rig rig;
    MixerPanel panel (rig.binding());
    panel.setSize (420, 520);
    auto& m = rig.engine.getMixer();
    m.setParameter ("ch0.fader", 0.25);
    m.setParameter ("ch1.trim", 3.0);
    m.setParameter ("ch0.pfl", 1.0);
    m.setParameter ("ch1.xfader", 2.0);
    m.setParameter ("master", 0.5);
    m.setParameter ("crossfader", 0.1);
    m.setParameter ("crossfader.curve", 0.0);
    m.setParameter ("cue.mix", 0.9);
    rig.recorder.calls.clear();

    panel.refresh (kFrame);
    EXPECT_NEAR (panel.getChannel (0).getFader().getValue(), 0.25, 1e-6);
    EXPECT_NEAR (panel.getChannel (1).getTrim().getValue(), 3.0, 1e-6);
    EXPECT_TRUE (panel.getChannel (0).getPfl().getToggleState());
    EXPECT_TRUE (panel.getChannel (1).getAssign (CrossfaderAssign::Right).getToggleState());
    EXPECT_NEAR (panel.getMaster().getFader().getValue(), 0.5, 1e-6);
    EXPECT_NEAR (panel.getCrossfader().getSlider().getValue(), 0.1, 1e-6);
    EXPECT_TRUE (panel.getCrossfader().getCurveButton (CrossfaderCurve::Linear).getToggleState());
    EXPECT_NEAR (panel.getCue().getMix().getValue(), 0.9, 1e-6);
    EXPECT_TRUE (rig.recorder.calls.empty()) << "mirroring must not echo back into the router";
}

TEST (MixerWidgetTest, ChannelMetersReadEngineLevelsPerSide)
{
    Rig rig;
    MixerPanel panel (rig.binding());
    panel.setSize (420, 520);
    rig.feedChannelZero (0.5F);
    panel.refresh (kFrame);

    auto& meter = panel.getChannel (0).getMeter();
    EXPECT_GT (meter.getLeft().getBallistics().level(), 0.5F);
    EXPECT_FLOAT_EQ (meter.getRight().getBallistics().level(), 0.0F) << "right input is silent";
    EXPECT_FLOAT_EQ (panel.getChannel (1).getMeter().getLeft().getBallistics().level(), 0.0F);
    EXPECT_GT (panel.getMaster().getMeter().getLeft().getBallistics().level(), 0.0F);
}

TEST (MixerWidgetTest, ClippingChannelLightsTheLampAndSilenceReleasesTheLevel)
{
    Rig rig;
    MixerPanel panel (rig.binding());
    panel.setSize (420, 520);
    rig.feedChannelZero (1.0F);
    panel.refresh (kFrame);
    EXPECT_TRUE (panel.getChannel (0).getMeter().getLeft().getBallistics().isClipping());

    rig.feedChannelZero (0.0F);
    for (int i = 0; i < 2000; ++i)   // the engine peak itself decays over about a second
    {
        rig.feedChannelZero (0.0F);
        panel.refresh (kFrame);
    }
    EXPECT_FLOAT_EQ (panel.getChannel (0).getMeter().getLeft().getBallistics().level(), 0.0F);
}

TEST (MixerWidgetTest, FxShortcutSwitchesPageAndScope)
{
    Rig rig;
    MixerPanel panel (rig.binding());
    panel.setSize (420, 520);
    EXPECT_EQ (panel.getPage(), MixerPanel::Page::Mixer);
    EXPECT_TRUE (panel.getChannel (0).isVisible());
    EXPECT_FALSE (panel.getFx().isVisible());

    panel.getChannel (1).getFxButton().onClick();
    EXPECT_EQ (panel.getPage(), MixerPanel::Page::Fx);
    EXPECT_EQ (panel.getFx().getScope(), 2) << "scope 2 is channel 1";
    EXPECT_TRUE (panel.getFx().isVisible());
    EXPECT_FALSE (panel.getChannel (0).isVisible());

    panel.getPageButton (MixerPanel::Page::Mixer).onClick();
    EXPECT_EQ (panel.getPage(), MixerPanel::Page::Mixer);
    EXPECT_TRUE (panel.getChannel (0).isVisible());
}

TEST (MixerWidgetTest, LayoutHasNoOverlapsAcrossSizesAndChannelCounts)
{
    for (int decks : { 1, 2, 4 })
    {
        Rig rig (decks);
        MixerPanel panel (rig.binding());
        for (auto [w, h] : { std::pair { 420, 520 }, std::pair { 560, 600 }, std::pair { 760, 700 } })
        {
            const auto context = juce::String (decks) + " decks " + juce::String (w) + "x" + juce::String (h);
            panel.setSize (w, h);
            std::vector<juce::Component*> parts { &panel.getMaster(), &panel.getCrossfader(), &panel.getCue() };
            for (int c = 0; c < panel.getNumChannels(); ++c)
                parts.push_back (&panel.getChannel (c));
            expectNoOverlap (parts, context);
            for (auto* p : parts)
                EXPECT_TRUE (panel.getLocalBounds().contains (p->getBounds()) && ! p->getBounds().isEmpty()) << context << " clipped";
        }
    }
}

TEST (MixerWidgetTest, StripChildrenFitAndDoNotOverlapAtTheDefaultSlotSize)
{
    Rig rig;
    GlassLookAndFeel lnf;
    MainView view (rig.engine, rig.recorder);
    view.setLookAndFeel (&lnf);
    auto panel = std::make_unique<MixerPanel> (rig.binding());
    auto* raw = panel.get();
    ASSERT_TRUE (view.setSlotContent (LayoutSpec::kMixerSlot, std::move (panel)));
    view.setSize (1280, 800);

    for (int c = 0; c < raw->getNumChannels(); ++c)
    {
        auto& strip = raw->getChannel (c);
        std::vector<juce::Component*> kids;
        for (auto* k : strip.getChildren())
            if (k->isVisible())
                kids.push_back (k);
        expectNoOverlap (kids, "strip " + juce::String (c));
        for (auto* k : kids)
            EXPECT_TRUE (strip.getLocalBounds().contains (k->getBounds()) && ! k->getBounds().isEmpty()) << "strip child clipped";
        // The fader needs real travel, not a sliver.
        EXPECT_GE (strip.getFader().getHeight(), 80);
    }
    constexpr int kTextMargin = 10;
    for (int c = 0; c < raw->getNumChannels(); ++c)
    {
        auto& strip = raw->getChannel (c);
        std::vector<juce::TextButton*> buttons { &strip.getPfl(), &strip.getFxButton(), &strip.getKill (0), &strip.getKill (1), &strip.getKill (2),
                                                  &strip.getAssign (CrossfaderAssign::Thru), &strip.getAssign (CrossfaderAssign::Left),
                                                  &strip.getAssign (CrossfaderAssign::Right) };
        for (auto* b : buttons)
        {
            juce::GlyphArrangement glyphs;
            glyphs.addLineOfText (lnf.getTextButtonFont (*b, b->getHeight()), b->getButtonText(), 0.0F, 0.0F);
            EXPECT_LE (static_cast<int> (std::ceil (glyphs.getBoundingBox (0, -1, true).getWidth())) + kTextMargin, b->getWidth())
                << b->getButtonText() << " would be truncated";
        }
    }
    view.setLookAndFeel (nullptr);
}

TEST (MixerWidgetTest, PanelPaintsOnBothPagesInBothThemes)
{
    Rig rig;
    for (const char* themeId : { "openmixer.dark", "openmixer.daylight" })
    {
        ThemeRegistry registry;
        registry.loadBuiltIns();
        ASSERT_TRUE (registry.select (themeId)) << themeId;
        GlassLookAndFeel lnf;
        MixerPanel panel (rig.binding());
        panel.setLookAndFeel (&lnf);
        rig.feedChannelZero (0.7F);
        panel.refresh (kFrame);
        EXPECT_TRUE (hasContent (render (panel, 420, 520))) << themeId << " mixer page";
        panel.setPage (MixerPanel::Page::Fx);
        EXPECT_TRUE (hasContent (render (panel, 420, 520))) << themeId << " fx page";
        panel.setLookAndFeel (nullptr);
    }
    ThemeRegistry restore;
    restore.loadBuiltIns();
    restore.select (ThemeRegistry::defaultId());   // the theme is process-global
}

TEST (MixerWidgetTest, MixerSlotAcceptsThePanelInTheMainView)
{
    Rig rig;
    MainView view (rig.engine, rig.recorder);
    ASSERT_TRUE (view.setSlotContent (LayoutSpec::kMixerSlot, std::make_unique<MixerPanel> (rig.binding())));
    view.setSize (1280, 800);
    auto* panel = dynamic_cast<MixerPanel*> (view.getSlot (LayoutSpec::kMixerSlot));
    ASSERT_NE (panel, nullptr);
    EXPECT_EQ (panel->getBounds(), view.getSlot (LayoutSpec::kMixerSlot)->getBounds());
    EXPECT_GE (panel->getWidth(), 360);
}

TEST (MixerWidgetTest, DebugPanelHostsDeckStripsAboveTheMixer)
{
    Rig rig;
    DebugPanel panel (rig.engine, rig.recorder);
    panel.setSize (900, 760);
    EXPECT_FALSE (panel.getMixer().getBounds().isEmpty());
    EXPECT_EQ (panel.getMixer().getNumChannels(), 2);
    panel.getMixer().getChannel (0).getFader().setValue (0.4, juce::sendNotificationSync);
    EXPECT_TRUE (rig.recorder.saw ("mixer.ch0.fader", 0.4));
    EXPECT_TRUE (hasContent (render (panel, 900, 760)));
}
