// OpenMixer SettingsPanelTest - settings pages reflect stored values and write edits back (audio model faked: no real device)
#include <JuceHeader.h>
#include "LibraryUiTestUtil.h"
#include "app/AppPreferencesTargets.h"
#include "ui/GlassLookAndFeel.h"
#include "ui/SettingsPanel.h"

#include <gtest/gtest.h>

#include <set>

using namespace openmixer;
using namespace openmixer::ui;

namespace
{
struct FakeAudioModel final : AudioSettingsModel
{
    juce::StringArray getDeviceTypes() const override { return { "ALSA", "JACK" }; }
    juce::String getDeviceType() const override { return type; }
    juce::String setDeviceType (const juce::String& t) override { type = t; calls.add ("type:" + t); return {}; }
    juce::StringArray getOutputDevices() const override { return { "Built-in", "USB DJ Interface" }; }
    juce::String getOutputDevice() const override { return device; }
    juce::String setOutputDevice (const juce::String& d) override { device = d; calls.add ("device:" + d); return error; }
    juce::Array<double> getSampleRates() const override { return { 44100.0, 48000.0, 96000.0 }; }
    double getSampleRate() const override { return rate; }
    juce::String setSampleRate (double r) override { rate = r; calls.add ("rate:" + juce::String (r)); return {}; }
    juce::Array<int> getBufferSizes() const override { return { 128, 256, 512 }; }
    int getBufferSize() const override { return buffer; }
    juce::String setBufferSize (int s) override { buffer = s; calls.add ("buffer:" + juce::String (s)); return {}; }
    int getNumOutputChannels() const override { return channels; }
    juce::String getStateXml() const override { return "<DEVICESETUP rate=\"" + juce::String (rate) + "\" buffer=\"" + juce::String (buffer) + "\"/>"; }

    juce::String type = "ALSA", device = "Built-in", error;
    double rate = 48000.0;
    int buffer = 256, channels = 4;
    juce::StringArray calls;
};

struct ThemeGuard
{
    ThemeTokens saved = theme::current();
    GlassLookAndFeel lnf;
    ThemeGuard() { juce::LookAndFeel::setDefaultLookAndFeel (&lnf); }
    ~ThemeGuard()
    {
        juce::LookAndFeel::setDefaultLookAndFeel (nullptr);
        theme::setCurrent (saved);
    }
};

int distinctPixels (const juce::Image& image)
{
    std::set<juce::uint32> seen;
    for (int y = 0; y < image.getHeight(); ++y)
        for (int x = 0; x < image.getWidth(); ++x)
            seen.insert (image.getPixelAt (x, y).getARGB());
    return static_cast<int> (seen.size());
}

juce::Array<juce::MidiDeviceInfo> twoInputs()
{
    return { juce::MidiDeviceInfo ("Launchpad", "id-1"), juce::MidiDeviceInfo ("DDJ Controller", "id-2") };
}

struct PanelRig
{
    PanelRig()
    {
        themes.loadBuiltIns();
        themes.select ("openmixer.dark");
    }

    std::unique_ptr<SettingsPanel> make (bool withAudio = true, LibraryServices* library = nullptr)
    {
        SettingsServices s { prefs, themes, withAudio ? &audio : nullptr,
                             [this] { return stats; }, [] { return twoInputs(); }, library,
                             [] { return juce::StringArray ("Gain Example", "Other"); }, [this] { ++reloads; } };
        auto panel = std::make_unique<SettingsPanel> (std::move (s));
        panel->onChanged = [this] { ++changes; };
        panel->setBounds (0, 0, 1000, 700);
        return panel;
    }

    Preferences prefs;
    ThemeRegistry themes;
    FakeAudioModel audio;
    LatencyStats stats;
    int changes = 0, reloads = 0;
};
}

//==============================================================================
TEST (SettingsAudioPageTest, ShowsTheModelValuesAndStoredPairs)
{
    PanelRig rig;
    rig.prefs.audio.masterPair = 1;
    rig.prefs.audio.cuePair = -1;
    AudioSettingsPage page (rig.prefs, rig.audio, [] { return LatencyStats(); });
    EXPECT_EQ (page.getTypeBox().getText(), "ALSA");
    EXPECT_EQ (page.getDeviceBox().getText(), "Built-in");
    EXPECT_EQ (page.getRateBox().getText(), "48000 Hz");
    EXPECT_EQ (page.getBufferBox().getText(), "256 samples");
    EXPECT_EQ (page.getMasterBox().getText(), "Out 3+4");
    EXPECT_EQ (page.getCueBox().getText(), "None");
    EXPECT_EQ (page.getMasterBox().getNumItems(), 2) << "4 channels = 2 pairs";
    EXPECT_EQ (page.getCueBox().getNumItems(), 3) << "None + 2 pairs";
}

TEST (SettingsAudioPageTest, ChangingTheRateCallsTheModelAndStoresTheDeviceState)
{
    PanelRig rig;
    AudioSettingsPage page (rig.prefs, rig.audio, nullptr);
    int changed = 0;
    page.changed = [&] { ++changed; };
    page.getRateBox().setSelectedItemIndex (2, juce::sendNotificationSync);
    EXPECT_EQ (rig.audio.calls.joinIntoString (";"), "rate:96000");
    EXPECT_TRUE (rig.prefs.audio.deviceState.contains ("rate=\"96000\""));
    EXPECT_EQ (changed, 1);

    page.getBufferBox().setSelectedItemIndex (0, juce::sendNotificationSync);
    EXPECT_EQ (rig.audio.buffer, 128);
    page.getDeviceBox().setSelectedItemIndex (1, juce::sendNotificationSync);
    EXPECT_EQ (rig.audio.device, "USB DJ Interface");
    page.getTypeBox().setSelectedItemIndex (1, juce::sendNotificationSync);
    EXPECT_EQ (rig.audio.type, "JACK");
    EXPECT_EQ (changed, 4);
}

TEST (SettingsAudioPageTest, ReflectingDoesNotTouchTheModelOrRaiseChanges)
{
    PanelRig rig;
    AudioSettingsPage page (rig.prefs, rig.audio, nullptr);
    int changed = 0;
    page.changed = [&] { ++changed; };
    page.reflect();
    EXPECT_EQ (changed, 0);
    EXPECT_EQ (rig.audio.calls.size(), 0);
}

TEST (SettingsAudioPageTest, PairChoicesAreStoredAndNoneMeansMinusOne)
{
    PanelRig rig;
    AudioSettingsPage page (rig.prefs, rig.audio, nullptr);
    int changed = 0;
    page.changed = [&] { ++changed; };
    page.getMasterBox().setSelectedId (2, juce::sendNotificationSync);   // Out 3+4
    page.getCueBox().setSelectedId (2, juce::sendNotificationSync);      // Out 1+2
    EXPECT_EQ (rig.prefs.audio.masterPair, 1);
    EXPECT_EQ (rig.prefs.audio.cuePair, 0);
    page.getCueBox().setSelectedId (1, juce::sendNotificationSync);      // None
    EXPECT_EQ (rig.prefs.audio.cuePair, -1);
    EXPECT_EQ (changed, 3);
}

TEST (SettingsAudioPageTest, StereoDeviceOffersOnePairAndNoCueToStoredDefault)
{
    PanelRig rig;
    rig.audio.channels = 2;   // the stored default cue pair (1) does not exist on this device
    AudioSettingsPage page (rig.prefs, rig.audio, nullptr);
    EXPECT_EQ (page.getMasterBox().getNumItems(), 1);
    EXPECT_EQ (page.getCueBox().getText(), "None");
    EXPECT_EQ (rig.prefs.audio.cuePair, 1) << "viewing must not rewrite the stored choice";
}

TEST (SettingsAudioPageTest, ShowsLatencyXrunsAndCpu)
{
    PanelRig rig;
    rig.stats.sampleRate = 48000.0;
    rig.stats.bufferSize = 256;
    rig.stats.outputLatencyMs = 5.3;
    rig.stats.roundTripLatencyMs = 10.7;
    rig.stats.cpuLoad = 0.25;
    rig.stats.maxCallbackUs = 900.0;
    rig.stats.budgetUs = 5333.0;
    rig.stats.xrunCount = 3;
    AudioSettingsPage page (rig.prefs, rig.audio, [&] { return rig.stats; });
    const auto text = page.getStatsLabel().getText();
    EXPECT_TRUE (text.contains ("Output 5.3 ms")) << text;
    EXPECT_TRUE (text.contains ("round trip 10.7 ms")) << text;
    EXPECT_TRUE (text.contains ("CPU 25%")) << text;
    EXPECT_TRUE (text.contains ("xruns 3")) << text;

    rig.stats.xrunCount = 4;
    page.refreshStats();
    EXPECT_TRUE (page.getStatsLabel().getText().contains ("xruns 4"));
}

TEST (SettingsAudioPageTest, WithoutADeviceTheLabelSaysSo)
{
    PanelRig rig;
    AudioSettingsPage page (rig.prefs, rig.audio, [] { return LatencyStats(); });
    EXPECT_EQ (page.getStatsLabel().getText(), "No audio device running");
}

TEST (SettingsAudioPageTest, ASetupErrorIsShownInsteadOfStats)
{
    PanelRig rig;
    rig.audio.error = "device busy";
    AudioSettingsPage page (rig.prefs, rig.audio, [] { return LatencyStats(); });
    page.getDeviceBox().setSelectedItemIndex (1, juce::sendNotificationSync);
    EXPECT_EQ (page.getError(), "device busy");
    EXPECT_EQ (page.getStatsLabel().getText(), "device busy");
}

//==============================================================================
TEST (SettingsMidiPageTest, ListsInputsAndReflectsStoredSelection)
{
    Preferences prefs;
    prefs.midi.enableAll = false;
    prefs.midi.enabledInputs = juce::StringArray ("id-2");
    prefs.midi.mapFile = "/maps/x.json";
    MidiSettingsPage page (prefs, [] { return twoInputs(); });
    page.setBounds (0, 0, 700, 400);
    ASSERT_EQ (page.getNumInputRows(), 2);
    EXPECT_FALSE (page.getUseAllToggle().getToggleState());
    EXPECT_FALSE (page.getInputToggle (0)->getToggleState());
    EXPECT_TRUE (page.getInputToggle (1)->getToggleState());
    EXPECT_EQ (page.getInputToggle (1)->getButtonText(), "DDJ Controller");
    EXPECT_EQ (page.getMapRow().getPath(), "/maps/x.json");
    EXPECT_TRUE (page.getMapRow().getDefaultButton().isEnabled());
}

TEST (SettingsMidiPageTest, TogglingInputsAndTheAllSwitchEditsThePreferences)
{
    Preferences prefs;
    MidiSettingsPage page (prefs, [] { return twoInputs(); });
    page.setBounds (0, 0, 700, 400);
    int changed = 0;
    page.changed = [&] { ++changed; };
    EXPECT_TRUE (prefs.midi.enableAll);
    EXPECT_FALSE (page.getInputToggle (0)->isEnabled()) << "per-input choice only applies when not using all";

    page.getUseAllToggle().setToggleState (false, juce::sendNotificationSync);
    EXPECT_FALSE (prefs.midi.enableAll);
    EXPECT_EQ (prefs.midi.enabledInputs.size(), 2) << "leaving 'all' starts from every present input";
    page.getInputToggle (0)->setToggleState (false, juce::sendNotificationSync);
    EXPECT_EQ (prefs.midi.enabledInputs.joinIntoString (","), "id-2");
    page.getInputToggle (0)->setToggleState (true, juce::sendNotificationSync);
    EXPECT_EQ (prefs.midi.enabledInputs.size(), 2);
    EXPECT_EQ (changed, 3);
}

TEST (SettingsMidiPageTest, MapFileChoiceAndReset)
{
    Preferences prefs;
    MidiSettingsPage page (prefs, [] { return twoInputs(); });
    EXPECT_FALSE (page.getMapRow().getDefaultButton().isEnabled());
    int changed = 0;
    page.changed = [&] { ++changed; };
    page.getMapRow().pick (juce::File ("/maps/ddj.json"));
    EXPECT_EQ (prefs.midi.mapFile, "/maps/ddj.json");
    EXPECT_EQ (page.getMapRow().getPath(), "/maps/ddj.json");
    page.getMapRow().getDefaultButton().onClick();
    EXPECT_EQ (prefs.midi.mapFile, juce::String());
    EXPECT_EQ (page.getMapRow().getPath(), Preferences::defaultMapFile().getFullPathName());
    EXPECT_EQ (changed, 2);
}

TEST (SettingsMidiPageTest, NoInputsStillBuilds)
{
    Preferences prefs;
    MidiSettingsPage page (prefs, [] { return juce::Array<juce::MidiDeviceInfo>(); });
    page.setBounds (0, 0, 700, 400);
    EXPECT_EQ (page.getNumInputRows(), 0);
}

//==============================================================================
TEST (SettingsLibraryPageTest, AddAndRemoveWatchFoldersThroughTheServices)
{
    library::test::TempDir dir;
    library::test::writeWav (dir.dir.getChildFile ("music/a.wav"), 1.0);
    auto options = libui::memoryOptions();
    options.dbPath = dir.dir.getChildFile("db/library.db").getFullPathName();
    options.scanOnStart = false;
    auto services = libui::makeServices (std::move (options));
    ASSERT_NE (services, nullptr);

    Preferences prefs;
    LibrarySettingsPage page (prefs, services.get());
    page.setBounds (0, 0, 700, 420);
    EXPECT_TRUE (page.getFolders().empty());
    EXPECT_TRUE (page.getAddButton().isEnabled());

    int notified = 0;
    struct Counter : LibraryServices::Listener
    {
        explicit Counter (int& n) : count (n) {}
        void libraryChanged() override { ++count; }
        int& count;
    } counter (notified);
    services->addListener (&counter);

    const auto folder = dir.dir.getChildFile ("music");
    ASSERT_TRUE (page.addFolder (folder).wasOk());
    ASSERT_EQ (page.getFolders().size(), 1u);
    EXPECT_EQ (page.getFolders()[0], folder.getFullPathName());
    EXPECT_GE (notified, 1) << "views refresh after the folder list changed";
    EXPECT_TRUE (libui::pumpUntil (*services, [&] { return services->getLibrary().tracks().count() == 1; })) << "the new folder is scanned";

    page.getList().selectRow (0);
    page.getRemoveButton().onClick();
    EXPECT_TRUE (page.getFolders().empty());
    EXPECT_TRUE (services->getWatchFolders()->list().empty());
    services->removeListener (&counter);
}

TEST (SettingsLibraryPageTest, RescanTriggersAScanPass)
{
    library::test::TempDir dir;
    auto options = libui::memoryOptions();
    options.dbPath = dir.dir.getChildFile ("library.db").getFullPathName();
    options.scanOnStart = false;
    auto services = libui::makeServices (std::move (options));
    ASSERT_NE (services, nullptr);
    Preferences prefs;
    LibrarySettingsPage page (prefs, services.get());
    const int before = services->getWatchFolders()->scanCount();
    page.getRescanButton().onClick();
    EXPECT_TRUE (services->getWatchFolders()->waitForScans (before + 1, 5000));
}

TEST (SettingsLibraryPageTest, IntervalAndScanOnStartAreStoredAndReflected)
{
    Preferences prefs;
    prefs.library = { false, 15 };
    LibrarySettingsPage page (prefs, nullptr);
    EXPECT_EQ (page.getIntervalBox().getSelectedId(), 15);
    EXPECT_FALSE (page.getScanOnStartToggle().getToggleState());
    int changed = 0;
    page.changed = [&] { ++changed; };
    page.getIntervalBox().setSelectedId (60, juce::sendNotificationSync);
    page.getScanOnStartToggle().setToggleState (true, juce::sendNotificationSync);
    EXPECT_EQ (prefs.library.scanIntervalMinutes, 60);
    EXPECT_TRUE (prefs.library.scanOnStart);
    EXPECT_EQ (changed, 2);
}

TEST (SettingsLibraryPageTest, HandEditedIntervalIsStillShown)
{
    Preferences prefs;
    prefs.library.scanIntervalMinutes = 7;
    LibrarySettingsPage page (prefs, nullptr);
    EXPECT_EQ (page.getIntervalBox().getText(), "Every 7 minutes");
}

TEST (SettingsLibraryPageTest, UnavailableLibraryDisablesTheFolderButtons)
{
    Preferences prefs;
    {
        LibrarySettingsPage page (prefs, nullptr);
        EXPECT_FALSE (page.getAddButton().isEnabled());
        EXPECT_FALSE (page.addFolder (juce::File ("/tmp")).wasOk());
        EXPECT_EQ (page.getStatusLabel().getText(), "The library is not available.");
    }
    auto memory = libui::makeServices();   // ":memory:" has no watch folders
    LibrarySettingsPage page (prefs, memory.get());
    EXPECT_FALSE (page.getRescanButton().isEnabled());
    EXPECT_FALSE (page.removeFolder ("/x").wasOk());
}

//==============================================================================
TEST (SettingsAppearancePageTest, ListsRegistryThemesAndSelectsTheStoredOne)
{
    PanelRig rig;
    rig.prefs.theme = "openmixer.daylight";
    AppearanceSettingsPage page (rig.prefs, rig.themes);
    EXPECT_EQ (page.getNumThemes(), 2);
    EXPECT_EQ (page.getThemeBox().getText(), "Daylight Frost");
}

TEST (SettingsAppearancePageTest, PickingAThemeStoresItsId)
{
    PanelRig rig;
    AppearanceSettingsPage page (rig.prefs, rig.themes);
    int changed = 0;
    page.changed = [&] { ++changed; };
    page.getThemeBox().setText ("Daylight Frost", juce::sendNotificationSync);
    EXPECT_EQ (rig.prefs.theme, "openmixer.daylight");
    EXPECT_EQ (changed, 1);
}

TEST (SettingsAppearancePageTest, ThemesAddedLaterAppearInThePicker)
{
    PanelRig rig;
    AppearanceSettingsPage page (rig.prefs, rig.themes);
    ASSERT_EQ (page.getNumThemes(), 2);
    ASSERT_TRUE (rig.themes.addJson ("{\"id\":\"plugin.neon\",\"name\":\"Neon\"}", ThemeRegistry::Source::Plugin, "neon-plugin").wasOk());
    rig.themes.dispatchPendingMessages();   // change messages are asynchronous
    EXPECT_EQ (page.getNumThemes(), 3);
    page.chooseTheme ("plugin.neon");
    EXPECT_EQ (page.getThemeBox().getText(), "Neon");
}

//==============================================================================
TEST (SettingsPluginsPageTest, FolderChoiceDefaultAndLoadedList)
{
    Preferences prefs;
    int reloads = 0;
    PluginsSettingsPage page (prefs, [] { return juce::StringArray ("Gain Example", "Other"); }, [&] { ++reloads; });
    EXPECT_EQ (page.getFolderRow().getPath(), Preferences::defaultPluginFolder().getFullPathName());
    EXPECT_FALSE (page.getFolderRow().getDefaultButton().isEnabled());
    EXPECT_EQ (page.getLoadedLabel().getText(), "Gain Example\nOther");

    int changed = 0;
    page.changed = [&] { ++changed; };
    page.getFolderRow().pick (juce::File ("/opt/plugins"));
    EXPECT_EQ (prefs.pluginFolder, "/opt/plugins");
    EXPECT_EQ (page.getFolderRow().getPath(), "/opt/plugins");
    page.getReloadButton().onClick();
    EXPECT_EQ (reloads, 1);
    page.getFolderRow().getDefaultButton().onClick();
    EXPECT_EQ (prefs.pluginFolder, juce::String());
    EXPECT_EQ (changed, 2);
}

//==============================================================================
TEST (SettingsPanelTest, BuildsOneTabPerPageAndOmitsAudioWithoutAModel)
{
    PanelRig rig;
    EXPECT_EQ (rig.make()->getNumPages(), 5);
    auto noAudio = rig.make (false);
    EXPECT_EQ (noAudio->getNumPages(), 4);
    EXPECT_EQ (noAudio->getTabButton (0).getButtonText(), "MIDI");
}

TEST (SettingsPanelTest, ShowsOnePageAtATime)
{
    PanelRig rig;
    auto panel = rig.make();
    EXPECT_EQ (panel->getCurrentPage(), 0);
    EXPECT_TRUE (panel->getPage (0)->isVisible());
    panel->getTabButton (3).onClick();
    EXPECT_EQ (panel->getCurrentPage(), 3);
    for (int i = 0; i < panel->getNumPages(); ++i)
        EXPECT_EQ (panel->getPage (i)->isVisible(), i == 3) << i;
    panel->showPage (99);   // ignored
    EXPECT_EQ (panel->getCurrentPage(), 3);
}

TEST (SettingsPanelTest, PageEditsAreForwardedToOnChanged)
{
    PanelRig rig;
    auto panel = rig.make();
    panel->showPage (3);
    dynamic_cast<AppearanceSettingsPage*> (panel->getPage (3))->chooseTheme ("openmixer.daylight");
    EXPECT_EQ (rig.changes, 1);
    EXPECT_EQ (rig.prefs.theme, "openmixer.daylight");
}

TEST (SettingsPanelTest, ReflectAllPicksUpPreferencesChangedElsewhere)
{
    PanelRig rig;
    auto panel = rig.make();
    rig.prefs.theme = "openmixer.daylight";
    rig.prefs.pluginFolder = "/else";
    rig.prefs.audio.masterPair = 1;
    panel->reflectAll();
    EXPECT_EQ (dynamic_cast<AppearanceSettingsPage*> (panel->getPage (3))->getThemeBox().getText(), "Daylight Frost");
    EXPECT_EQ (dynamic_cast<PluginsSettingsPage*> (panel->getPage (4))->getFolderRow().getPath(), "/else");
    EXPECT_EQ (dynamic_cast<AudioSettingsPage*> (panel->getPage (0))->getMasterBox().getText(), "Out 3+4");
    EXPECT_EQ (rig.changes, 0);
}

TEST (SettingsPanelTest, CloseButtonEscapeAndClickOutsideDismiss)
{
    PanelRig rig;
    auto panel = rig.make();
    int closed = 0;
    panel->onClose = [&] { ++closed; };
    panel->getCloseButton().onClick();
    EXPECT_EQ (closed, 1);
    EXPECT_TRUE (panel->keyPressed (juce::KeyPress (juce::KeyPress::escapeKey)));
    EXPECT_FALSE (panel->keyPressed (juce::KeyPress ('a')));
    EXPECT_EQ (closed, 2);

    const auto card = panel->getCardBounds();
    const auto event = [&] (juce::Point<int> p)
    {
        return juce::MouseEvent (juce::Desktop::getInstance().getMainMouseSource(), p.toFloat(), {}, 0.0F, 0.0F, 0.0F, 0.0F, 0.0F,
                                 panel.get(), panel.get(), juce::Time::getCurrentTime(), p.toFloat(), juce::Time::getCurrentTime(), 1, false);
    };
    panel->mouseDown (event (card.getCentre()));
    EXPECT_EQ (closed, 2) << "a click inside the card keeps it open";
    panel->mouseDown (event ({ 2, 2 }));
    EXPECT_EQ (closed, 3);
}

TEST (SettingsPanelTest, CardFitsTheWindowAndShrinksInSmallWindows)
{
    PanelRig rig;
    auto panel = rig.make();
    EXPECT_TRUE (panel->getLocalBounds().contains (panel->getCardBounds()));
    EXPECT_EQ (panel->getCardBounds().getWidth(), theme::settingsPanelWidth());
    EXPECT_EQ (panel->getCardBounds().getCentre(), panel->getLocalBounds().getCentre());
    panel->setBounds (0, 0, 500, 400);
    EXPECT_TRUE (panel->getLocalBounds().contains (panel->getCardBounds()));
    EXPECT_LT (panel->getCardBounds().getWidth(), theme::settingsPanelWidth());
}

TEST (SettingsPanelTest, EveryTabLaysOutInsideTheCardWithoutOverlap)
{
    ThemeGuard guard;
    PanelRig rig;
    auto panel = rig.make();
    for (int p = 0; p < panel->getNumPages(); ++p)
    {
        panel->showPage (p);
        auto* page = panel->getPage (p);
        const auto pageBounds = page->getBounds();
        EXPECT_TRUE (panel->getCardBounds().contains (pageBounds)) << p;
        for (int i = 0; i < page->getNumChildComponents(); ++i)
        {
            const auto a = page->getChildComponent (i)->getBounds();
            EXPECT_TRUE (page->getLocalBounds().contains (a)) << "page " << p << " child " << i;
            for (int j = i + 1; j < page->getNumChildComponents(); ++j)
                EXPECT_FALSE (a.intersects (page->getChildComponent (j)->getBounds())) << "page " << p << ": " << i << " vs " << j;
        }
    }
}

TEST (SettingsPanelTest, PluginsCanAddTheirOwnPage)
{
    struct CustomPage final : SettingsPage { void reflect() override { ++reflected; } int reflected = 0; };
    PanelRig rig;
    auto panel = rig.make();
    auto page = std::make_unique<CustomPage>();
    auto* raw = page.get();
    panel->addPage ("Extra", std::move (page));
    EXPECT_EQ (panel->getNumPages(), 6);
    panel->reflectAll();
    EXPECT_EQ (raw->reflected, 1);
    raw->changed();
    EXPECT_EQ (rig.changes, 1);
}

TEST (SettingsPanelTest, RendersEveryTabInBothThemes)
{
    ThemeGuard guard;
    PanelRig rig;
    auto services = libui::makeServices();
    auto panel = rig.make (true, services.get());
    for (const auto* id : { "openmixer.dark", "openmixer.daylight" })
    {
        ASSERT_TRUE (rig.themes.select (id));
        for (int p = 0; p < panel->getNumPages(); ++p)
        {
            panel->showPage (p);
            EXPECT_GT (distinctPixels (panel->createComponentSnapshot (panel->getLocalBounds())), 20) << id << " tab " << p;
        }
    }
}

TEST (SettingsAudioPageTest, OneUiDeviceChangeIsOneDeviceChangeNotTwo)
{
    PanelRig rig;
    int inits = 0;
    AudioDeviceControl control { [&] { return rig.audio.getStateXml(); },
                                 [&] (const juce::XmlElement&) { ++inits; return juce::String(); } };
    juce::AudioDeviceManager dm;   // never initialised
    MidiMapper mapper;
    MidiInputRouter midi { dm, mapper };
    PluginRegistry plugins;
    OutputRouter outputs;
    AppPreferencesTargets targets (control, midi, mapper, plugins, rig.themes, outputs);
    PreferencesApplier applier (targets);
    rig.prefs.audio.deviceState = rig.audio.getStateXml();
    targets.noteAudioRequested (rig.prefs.audio.deviceState);   // start-up: the app opened the device itself
    applier.apply (rig.prefs);
    EXPECT_EQ (inits, 0);

    AudioSettingsPage page (rig.prefs, rig.audio, nullptr);
    page.changed = [&] { applier.apply (rig.prefs); };   // what the app does on every settings change
    page.getRateBox().setSelectedItemIndex (2, juce::sendNotificationSync);
    EXPECT_EQ (rig.audio.calls.size(), 1) << "the model changed the device once";
    EXPECT_EQ (inits, 0) << "the applier must not initialise the device again";
    page.getBufferBox().setSelectedItemIndex (0, juce::sendNotificationSync);
    EXPECT_EQ (inits, 0);
}
