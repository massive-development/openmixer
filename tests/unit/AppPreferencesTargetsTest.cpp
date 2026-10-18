// OpenMixer AppPreferencesTargetsTest - the app's subsystem glue, headless: plugin folder + theme forwarding, device init
#include <JuceHeader.h>
#include "LibraryIngestTestUtil.h"
#include "app/AppPreferencesTargets.h"
#include "ui/Theme.h"

#include <gtest/gtest.h>

using namespace openmixer;
using ui::ThemeRegistry;

namespace
{
constexpr const char* kExampleTheme = "org.openmixer.example.theme";

struct Rig
{
    Rig()
    {
        themes.loadBuiltIns();
        themes.select (ThemeRegistry::defaultId());
        plugins.bindRouter (&router);
    }
    ~Rig() { plugins.unloadAll(); ui::theme::setCurrent (saved); }

    // Fresh device-control fake each time: counts initialisations, reports a fixed current state.
    AudioDeviceControl control()
    {
        return { [this] { return deviceState; },
                 [this] (const juce::XmlElement&) { ++inits; return juce::String(); } };
    }

    ui::ThemeTokens saved = ui::theme::current();
    ThemeRegistry themes;
    ParameterRouter router;
    MidiMapper mapper;
    juce::AudioDeviceManager dm;   // never initialised: no device is opened
    MidiInputRouter midi { dm, mapper };
    PluginRegistry plugins;
    OutputRouter outputs;
    juce::String deviceState = "<DEVICESETUP a=\"1\"/>";
    int inits = 0;
};

juce::File pluginFolderWithExample (const juce::File& root)
{
    const juce::File lib (OPENMIXER_EXAMPLE_PLUGIN_PATH);
    auto dir = root.getChildFile ("plugins");
    dir.createDirectory();
    EXPECT_TRUE (lib.copyFileTo (dir.getChildFile (lib.getFileName())));
    return dir;
}
}

TEST (AppPreferencesTargetsTest, PluginFolderLoadsPluginsAndForwardsTheirThemes)
{
    library::test::TempDir tmp;
    Rig rig;
    AppPreferencesTargets targets (rig.control(), rig.midi, rig.mapper, rig.plugins, rig.themes, rig.outputs);
    EXPECT_EQ (rig.themes.find (kExampleTheme), nullptr);

    targets.applyPluginFolder (pluginFolderWithExample (tmp.dir));
    EXPECT_EQ (rig.plugins.getPluginIds().size(), 1);
    const auto* entry = rig.themes.find (kExampleTheme);
    ASSERT_NE (entry, nullptr);
    EXPECT_EQ (entry->source, ThemeRegistry::Source::Plugin);
    EXPECT_EQ (entry->owner, "org.openmixer.example.gain");
    EXPECT_TRUE (rig.router.setParameter ("example.gain", 0.5)) << "plugin parameter targets follow the bound router";
}

TEST (AppPreferencesTargetsTest, EmptyFolderRemovesPluginThemesButKeepsUserAndBuiltInOnes)
{
    library::test::TempDir tmp;
    Rig rig;
    ASSERT_TRUE (rig.themes.addJson ("{\"id\":\"user.mine\",\"name\":\"Mine\"}", ThemeRegistry::Source::User, "mine.json").wasOk());
    AppPreferencesTargets targets (rig.control(), rig.midi, rig.mapper, rig.plugins, rig.themes, rig.outputs);
    targets.applyPluginFolder (pluginFolderWithExample (tmp.dir));
    ASSERT_NE (rig.themes.find (kExampleTheme), nullptr);

    const auto empty = tmp.dir.getChildFile ("empty");
    empty.createDirectory();
    targets.applyPluginFolder (empty);
    EXPECT_EQ (rig.themes.find (kExampleTheme), nullptr);
    EXPECT_EQ (rig.plugins.getPluginIds().size(), 0);
    EXPECT_NE (rig.themes.find ("user.mine"), nullptr);
    EXPECT_NE (rig.themes.find (ThemeRegistry::defaultId()), nullptr);
}

TEST (AppPreferencesTargetsTest, ActivePluginThemeFallsBackWhenItsPluginGoesAndReturnsOnReload)
{
    library::test::TempDir tmp;
    Rig rig;
    AppPreferencesTargets targets (rig.control(), rig.midi, rig.mapper, rig.plugins, rig.themes, rig.outputs);
    const auto withPlugin = pluginFolderWithExample (tmp.dir);
    targets.applyPluginFolder (withPlugin);
    ASSERT_TRUE (targets.applyTheme (kExampleTheme));
    EXPECT_EQ (rig.themes.getActiveId(), kExampleTheme);

    const auto empty = tmp.dir.getChildFile ("empty");
    empty.createDirectory();
    targets.applyPluginFolder (empty);
    EXPECT_EQ (rig.themes.getActiveId(), ThemeRegistry::defaultId()) << "the plugin theme vanished: fall back";

    targets.applyPluginFolder (withPlugin);
    EXPECT_EQ (rig.themes.getActiveId(), kExampleTheme) << "the chosen theme is re-selected once it is back";
    targets.reloadPlugins();   // the Plugins tab's Reload button
    EXPECT_EQ (rig.themes.getActiveId(), kExampleTheme);
    EXPECT_FALSE (targets.applyTheme ("no.such.theme"));
    EXPECT_EQ (rig.themes.getActiveId(), kExampleTheme) << "an unknown id keeps the current theme";
}

TEST (AppPreferencesTargetsTest, SessionThemeOverrideSurvivesAPluginReload)
{
    library::test::TempDir tmp;
    Rig rig;
    AppPreferencesTargets targets (rig.control(), rig.midi, rig.mapper, rig.plugins, rig.themes, rig.outputs);
    const auto withPlugin = pluginFolderWithExample (tmp.dir);
    targets.applyPluginFolder (withPlugin);
    ASSERT_TRUE (targets.applyTheme (ThemeRegistry::defaultId()));   // the saved preference
    ASSERT_TRUE (targets.applyTheme (kExampleTheme));                // OPENMIXER_THEME, applied by the app after it

    targets.reloadPlugins();
    EXPECT_EQ (rig.themes.getActiveId(), kExampleTheme) << "the override wins over the saved theme after a reload";
}

TEST (AppPreferencesTargetsTest, AudioStateIsInitialisedOnlyWhenItDiffersFromTheDevice)
{
    Rig rig;
    AppPreferencesTargets targets (rig.control(), rig.midi, rig.mapper, rig.plugins, rig.themes, rig.outputs);
    ui::AudioPrefs audio;
    audio.masterPair = 1;
    audio.cuePair = -1;
    audio.deviceState = rig.deviceState;   // what the UI stores after it changed the device itself
    targets.applyAudio (audio);
    EXPECT_EQ (rig.inits, 0);
    EXPECT_EQ (rig.outputs.getMasterPair(), 1);
    EXPECT_EQ (rig.outputs.getCuePair(), -1);

    audio.deviceState = "<DEVICESETUP a=\"2\"/>";   // a different state from elsewhere (hand edit)
    targets.applyAudio (audio);
    targets.applyAudio (audio);
    EXPECT_EQ (rig.inits, 1) << "applied once, then remembered";
}

TEST (AppPreferencesTargetsTest, StartUpStateRequestedByTheAppIsNotInitialisedAgain)
{
    Rig rig;
    AppPreferencesTargets targets (rig.control(), rig.midi, rig.mapper, rig.plugins, rig.themes, rig.outputs);
    ui::AudioPrefs audio;
    audio.deviceState = "<DEVICESETUP saved=\"1\"/>";   // differs from the device's reported state
    targets.noteAudioRequested (audio.deviceState);
    targets.applyAudio (audio);
    EXPECT_EQ (rig.inits, 0);
}

TEST (AppPreferencesTargetsTest, MidiMapIsLoadedBeforeInputsAreEnabledAndOnlyWhenItChanges)
{
    library::test::TempDir tmp;
    Rig rig;
    AppPreferencesTargets targets (rig.control(), rig.midi, rig.mapper, rig.plugins, rig.themes, rig.outputs);
    const auto map = tmp.dir.getChildFile ("map.json");
    map.replaceWithText (R"({"mappings":[{"type":"cc","number":7,"target":"a"}]})");
    ui::MidiPrefs prefs;
    prefs.mapFile = map.getFullPathName();
    targets.applyMidi (prefs);
    EXPECT_EQ (rig.mapper.getNumMappings(), 1u);

    map.replaceWithText (R"({"mappings":[]})");   // same path: not reloaded
    targets.applyMidi (prefs);
    EXPECT_EQ (rig.mapper.getNumMappings(), 1u);
}
