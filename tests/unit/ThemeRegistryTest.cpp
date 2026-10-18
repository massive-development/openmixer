// OpenMixer ThemeRegistryTest - built-ins, user dir, plugin themes, precedence and live switching
#include <JuceHeader.h>
#include "plugins/PluginRegistry.h"
#include "ui/GlassLookAndFeel.h"
#include "ui/Theme.h"
#include "ui/ThemeRegistry.h"

#include <gtest/gtest.h>

using namespace openmixer;
using namespace openmixer::ui;

namespace
{
juce::String themeJson (const juce::String& id, const juce::String& backdropTop, const juce::String& name = "T")
{
    return "{ \"id\": \"" + id + "\", \"name\": \"" + name + "\", \"colours\": { \"backdropTop\": \"" + backdropTop + "\" } }";
}

// Leaves the global theme as it found it, whatever the test did.
struct ThemeGuard
{
    ThemeTokens saved = theme::current();
    ~ThemeGuard() { theme::setCurrent (saved); }
};

struct ScopedDir
{
    juce::File dir = juce::File::getSpecialLocation (juce::File::tempDirectory).getNonexistentChildFile ("omthemes", "");
    ScopedDir() { dir.createDirectory(); }
    ~ScopedDir() { dir.deleteRecursively(); }
};

class ThemePlugin : public Plugin
{
public:
    PluginInfo info() const override { return { "test.theme", "Theme plugin", "1" }; }
    void onLoad (PluginHost& host) override
    {
        good = host.registerTheme (themeJson ("plugin.sky", "#FF102030"));
        duplicate = host.registerTheme (themeJson ("plugin.sky", "#FF405060"));
        bad = host.registerTheme ("{ nope");
        noId = host.registerTheme (R"({ "name": "x" })");
    }
    bool good = false, duplicate = true, bad = true, noId = true;
};
}

TEST (ThemeRegistryTest, LoadsBuiltInDarkAndLight)
{
    ThemeRegistry reg;
    EXPECT_EQ (reg.loadBuiltIns(), 2);
    const auto* dark = reg.find (ThemeRegistry::defaultId());
    ASSERT_NE (dark, nullptr);
    EXPECT_EQ (dark->source, ThemeRegistry::Source::BuiltIn);
    const auto* light = reg.find ("openmixer.daylight");
    ASSERT_NE (light, nullptr);
    EXPECT_GT (light->tokens.backdropTop.getBrightness(), 0.8F);
    EXPECT_LT (dark->tokens.backdropTop.getBrightness(), 0.2F);
}

TEST (ThemeRegistryTest, BuiltInDarkJsonMatchesTheCompiledDefault)
{
    ThemeRegistry reg;
    reg.loadBuiltIns();
    const auto& json = reg.find (ThemeRegistry::defaultId())->tokens;
    const auto& compiled = ThemeTokens::builtinDark();
    EXPECT_EQ (json.backdropTop, compiled.backdropTop);
    EXPECT_EQ (json.surface, compiled.surface);
    EXPECT_EQ (json.textMuted, compiled.textMuted);
    EXPECT_EQ (json.capBottom, compiled.capBottom);
    EXPECT_EQ (json.accents, compiled.accents);
    EXPECT_EQ (json.gap, compiled.gap);
    EXPECT_FLOAT_EQ (json.typeMono, compiled.typeMono);
    EXPECT_EQ (json.waveLow, compiled.waveLow);
    EXPECT_EQ (json.waveMid, compiled.waveMid);
    EXPECT_EQ (json.waveHigh, compiled.waveHigh);
    EXPECT_FLOAT_EQ (json.deckHeaderHeight, compiled.deckHeaderHeight);
    EXPECT_FLOAT_EQ (json.deckRowHeight, compiled.deckRowHeight);
    EXPECT_FLOAT_EQ (json.deckPitchWidth, compiled.deckPitchWidth);
    EXPECT_FLOAT_EQ (json.gridBarAlpha, compiled.gridBarAlpha);
    EXPECT_FLOAT_EQ (json.loopIdleAlpha, compiled.loopIdleAlpha);
    EXPECT_FLOAT_EQ (json.mixerKnobHeight, compiled.mixerKnobHeight);
    EXPECT_FLOAT_EQ (json.mixerTitleHeight, compiled.mixerTitleHeight);
    EXPECT_FLOAT_EQ (json.mixerFaderWidth, compiled.mixerFaderWidth);
    EXPECT_FLOAT_EQ (json.mixerCurveWidth, compiled.mixerCurveWidth);
    EXPECT_FLOAT_EQ (json.mixerFxBoxWidth, compiled.mixerFxBoxWidth);
    EXPECT_FLOAT_EQ (json.mixerFxEnableWidth, compiled.mixerFxEnableWidth);
    EXPECT_FLOAT_EQ (json.mixerPageButtonWidth, compiled.mixerPageButtonWidth);
    EXPECT_FLOAT_EQ (json.mixerPadX, compiled.mixerPadX);
    EXPECT_FLOAT_EQ (json.mixerPadY, compiled.mixerPadY);
    EXPECT_FLOAT_EQ (json.mixerAccentDot, compiled.mixerAccentDot);
    EXPECT_FLOAT_EQ (json.meterHoldTickHeight, compiled.meterHoldTickHeight);
    EXPECT_FLOAT_EQ (json.mixerMasterWidth, compiled.mixerMasterWidth);
    EXPECT_FLOAT_EQ (json.mixerCueHeight, compiled.mixerCueHeight);
    EXPECT_FLOAT_EQ (json.meterFloorDb, compiled.meterFloorDb);
    EXPECT_FLOAT_EQ (json.meterMidDb, compiled.meterMidDb);
    EXPECT_FLOAT_EQ (json.meterHighDb, compiled.meterHighDb);
    EXPECT_FLOAT_EQ (json.meterClipLevel, compiled.meterClipLevel);
    EXPECT_FLOAT_EQ (json.meterReleaseSeconds, compiled.meterReleaseSeconds);
    EXPECT_FLOAT_EQ (json.meterHoldSeconds, compiled.meterHoldSeconds);
    EXPECT_FLOAT_EQ (json.meterClipSeconds, compiled.meterClipSeconds);
    EXPECT_FLOAT_EQ (json.meterSegment, compiled.meterSegment);
}

TEST (ThemeRegistryTest, PrecedenceIsUserOverPluginOverBuiltIn)
{
    ThemeRegistry reg;
    ASSERT_TRUE (reg.addJson (themeJson ("x", "#FF000001"), ThemeRegistry::Source::BuiltIn, "b").wasOk());
    ASSERT_TRUE (reg.addJson (themeJson ("x", "#FF000002"), ThemeRegistry::Source::User, "u").wasOk());
    ASSERT_TRUE (reg.addJson (themeJson ("x", "#FF000003"), ThemeRegistry::Source::Plugin, "p").wasOk());
    EXPECT_EQ (reg.find ("x")->tokens.backdropTop, juce::Colour (0xFF000002));
    EXPECT_EQ (reg.getThemes().size(), 1U);   // one winner per id

    reg.removeSource (ThemeRegistry::Source::User);
    EXPECT_EQ (reg.find ("x")->tokens.backdropTop, juce::Colour (0xFF000003));
    reg.removeOwner ("p");
    EXPECT_EQ (reg.find ("x")->tokens.backdropTop, juce::Colour (0xFF000001));
}

TEST (ThemeRegistryTest, LaterAddWithinASourceWins)
{
    ThemeRegistry reg;
    reg.addJson (themeJson ("x", "#FF000001"), ThemeRegistry::Source::User, "a.json");
    reg.addJson (themeJson ("x", "#FF000002"), ThemeRegistry::Source::User, "b.json");
    EXPECT_EQ (reg.find ("x")->tokens.backdropTop, juce::Colour (0xFF000002));
    reg.addJson (themeJson ("x", "#FF000003"), ThemeRegistry::Source::User, "a.json");   // same owner replaces itself
    EXPECT_EQ (reg.find ("x")->tokens.backdropTop, juce::Colour (0xFF000003));
}

TEST (ThemeRegistryTest, InvalidJsonIsRejectedAndLeavesRegistryAlone)
{
    ThemeRegistry reg;
    reg.loadBuiltIns();
    EXPECT_TRUE (reg.addJson ("{ nope", ThemeRegistry::Source::User).failed());
    EXPECT_EQ (reg.getThemes().size(), 2U);
}

TEST (ThemeRegistryTest, UserDirectoryLoadsValidFilesAndReportsBadOnes)
{
    ScopedDir tmp;
    ASSERT_TRUE (tmp.dir.getChildFile ("a.json").replaceWithText (themeJson ("user.a", "#FF0A0B0C", "Aye")));
    ASSERT_TRUE (tmp.dir.getChildFile ("broken.json").replaceWithText ("{ nope"));
    ASSERT_TRUE (tmp.dir.getChildFile ("notes.txt").replaceWithText ("ignored"));

    ThemeRegistry reg;
    juce::StringArray errors;
    EXPECT_EQ (reg.loadUserDirectory (tmp.dir, &errors), 1);
    ASSERT_EQ (errors.size(), 1);
    EXPECT_TRUE (errors[0].startsWith ("broken.json"));
    ASSERT_NE (reg.find ("user.a"), nullptr);
    EXPECT_EQ (reg.find ("user.a")->info.name, "Aye");

    tmp.dir.getChildFile ("a.json").deleteFile();   // a rescan drops themes whose file is gone
    EXPECT_EQ (reg.loadUserDirectory (tmp.dir), 0);
    EXPECT_EQ (reg.find ("user.a"), nullptr);
    EXPECT_EQ (reg.loadUserDirectory (tmp.dir.getChildFile ("missing")), 0);   // absent dir is fine
}

TEST (ThemeRegistryTest, UserOverridesABuiltInById)
{
    ScopedDir tmp;
    tmp.dir.getChildFile ("mine.json").replaceWithText (themeJson (ThemeRegistry::defaultId(), "#FF123456", "My dark"));
    ThemeRegistry reg;
    reg.loadBuiltIns();
    reg.loadUserDirectory (tmp.dir);
    EXPECT_EQ (reg.find (ThemeRegistry::defaultId())->info.name, "My dark");
    EXPECT_EQ (reg.find (ThemeRegistry::defaultId())->source, ThemeRegistry::Source::User);
}

TEST (ThemeRegistryTest, SelectUnknownIdChangesNothing)
{
    ThemeGuard guard;
    ThemeRegistry reg;
    reg.loadBuiltIns();
    EXPECT_FALSE (reg.select ("nope"));
    EXPECT_TRUE (reg.getActiveId().isEmpty());
}

TEST (ThemeRegistryTest, SelectSwitchesCurrentThemeAndNotifiesListeners)
{
    ThemeGuard guard;
    struct Counter : theme::ThemeListener
    {
        int n = 0;
        void themeChanged() override { ++n; }
    } counter;
    theme::addListener (&counter);

    ThemeRegistry reg;
    reg.loadBuiltIns();
    ASSERT_TRUE (reg.select ("openmixer.daylight"));
    EXPECT_EQ (counter.n, 1);
    EXPECT_EQ (theme::backdropTop(), reg.find ("openmixer.daylight")->tokens.backdropTop);
    EXPECT_EQ (theme::accentFor (0), reg.find ("openmixer.daylight")->tokens.accents[0]);
    ASSERT_TRUE (reg.select (ThemeRegistry::defaultId()));
    EXPECT_EQ (theme::backdropTop(), ThemeTokens::builtinDark().backdropTop);
    theme::removeListener (&counter);
}

TEST (ThemeRegistryTest, AccentWrapsAndSurfaceOpacityApplies)
{
    ThemeGuard guard;
    auto t = ThemeTokens::builtinDark();
    t.surfaceOpacity = 0.5F;
    theme::setCurrent (t);
    EXPECT_EQ (theme::accentFor (4), theme::accentFor (0));
    EXPECT_EQ (theme::accentFor (-1), theme::accentFor (3));
    EXPECT_NEAR (theme::surface().getFloatAlpha(), t.surface.getFloatAlpha() * 0.5F, 0.01F);
    t.accents.clear();   // an empty palette must not crash accentFor
    theme::setCurrent (t);
    EXPECT_NO_THROW (theme::accentFor (3));
}

TEST (ThemeRegistryTest, RemovingTheActiveThemeFallsBackToDefault)
{
    ThemeGuard guard;
    ThemeRegistry reg;
    reg.loadBuiltIns();
    reg.addJson (themeJson ("temp", "#FF010101"), ThemeRegistry::Source::User, "t");
    ASSERT_TRUE (reg.select ("temp"));
    EXPECT_EQ (theme::backdropTop(), juce::Colour (0xFF010101));
    reg.removeOwner ("t");
    EXPECT_EQ (reg.getActiveId(), ThemeRegistry::defaultId());
    EXPECT_EQ (theme::backdropTop(), ThemeTokens::builtinDark().backdropTop);
}

TEST (ThemeRegistryTest, EditingTheActiveThemeReappliesIt)
{
    ThemeGuard guard;
    ThemeRegistry reg;
    reg.addJson (themeJson ("live", "#FF010101"), ThemeRegistry::Source::User, "f");
    reg.select ("live");
    reg.addJson (themeJson ("live", "#FF020202"), ThemeRegistry::Source::User, "f");
    EXPECT_EQ (theme::backdropTop(), juce::Colour (0xFF020202));
}

TEST (ThemeRegistryTest, CodeCanAddThemesDirectly)
{
    ThemeRegistry reg;
    auto t = ThemeTokens::builtinDark();
    t.gap = 20;
    reg.add ({ "code.theme", "Code", "dev" }, t, ThemeRegistry::Source::Plugin, "my.plugin");
    ASSERT_NE (reg.find ("code.theme"), nullptr);
    EXPECT_EQ (reg.find ("code.theme")->tokens.gap, 20);
}

TEST (ThemeRegistryTest, PluginThemesFlowThroughToTheRegistry)
{
    PluginRegistry plugins;
    auto plugin = std::make_unique<ThemePlugin>();
    auto* raw = plugin.get();
    ASSERT_TRUE (plugins.addPlugin (std::move (plugin)).wasOk());
    EXPECT_TRUE (raw->good);
    EXPECT_FALSE (raw->duplicate);
    EXPECT_FALSE (raw->bad);
    EXPECT_FALSE (raw->noId);

    ThemeRegistry reg;   // the app-level glue: forward what plugins registered
    for (const auto& t : plugins.getThemes())
        EXPECT_TRUE (reg.addJson (t.json, ThemeRegistry::Source::Plugin, t.owner).wasOk());
    ASSERT_NE (reg.find ("plugin.sky"), nullptr);
    EXPECT_EQ (reg.find ("plugin.sky")->owner, "test.theme");

    plugins.unload ("test.theme");
    EXPECT_TRUE (plugins.getThemes().empty());
    reg.removeOwner ("test.theme");
    EXPECT_EQ (reg.find ("plugin.sky"), nullptr);
}

TEST (ThemeRegistryTest, SwitchingUpdatesLookAndFeelColours)
{
    ThemeGuard guard;
    GlassLookAndFeel lnf;
    ThemeRegistry reg;
    reg.loadBuiltIns();
    reg.select (ThemeRegistry::defaultId());
    const auto darkText = lnf.findColour (juce::Label::textColourId);
    const auto darkBg = lnf.findColour (juce::ResizableWindow::backgroundColourId);
    EXPECT_EQ (lnf.findColour (juce::Slider::rotarySliderFillColourId), theme::accentFor (0));

    reg.select ("openmixer.daylight");
    EXPECT_NE (lnf.findColour (juce::Label::textColourId), darkText);
    EXPECT_NE (lnf.findColour (juce::ResizableWindow::backgroundColourId), darkBg);
    EXPECT_EQ (lnf.findColour (juce::ResizableWindow::backgroundColourId), reg.find ("openmixer.daylight")->tokens.backdropTop);
    EXPECT_EQ (lnf.findColour (juce::Slider::rotarySliderFillColourId), reg.find ("openmixer.daylight")->tokens.accents[0]);
    EXPECT_EQ (lnf.findColour (juce::PopupMenu::backgroundColourId), reg.find ("openmixer.daylight")->tokens.popup);
    EXPECT_EQ (lnf.findColour (juce::TextEditor::focusedOutlineColourId), theme::accentFor (0));
    EXPECT_EQ (lnf.findColour (juce::TextButton::textColourOnId), reg.find ("openmixer.daylight")->tokens.onAccent);
}

TEST (ThemeRegistryTest, ThemeSwitchReachesLiveComponents)
{
    ThemeGuard guard;
    GlassLookAndFeel lnf;
    struct Probe : juce::Component
    {
        int changes = 0;
        void lookAndFeelChanged() override { ++changes; }
    } probe;
    probe.setLookAndFeel (&lnf);
    probe.addToDesktop (juce::ComponentPeer::windowIsTemporary);   // headless-safe: no peer needed for the check below
    const int before = probe.changes;

    ThemeRegistry reg;
    reg.loadBuiltIns();
    reg.select ("openmixer.daylight");
    EXPECT_GT (probe.changes, before);   // sendLookAndFeelChange reached the top-level component
    probe.removeFromDesktop();
    probe.setLookAndFeel (nullptr);
}
