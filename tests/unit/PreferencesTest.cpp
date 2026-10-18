// OpenMixer PreferencesTest - versioned JSON model, persistence and the applier (subsystems mocked)
#include <JuceHeader.h>
#include "LibraryIngestTestUtil.h"
#include "ui/Preferences.h"
#include "ui/PreferencesApplier.h"

#include <gtest/gtest.h>

using namespace openmixer::ui;

namespace
{
Preferences customised()
{
    Preferences p;
    p.theme = "openmixer.daylight";
    p.audio = { "<DEVICESETUP deviceType=\"ALSA\" audioOutputDeviceName=\"hw:1\"/>", 2, -1 };
    p.midi.enableAll = false;
    p.midi.enabledInputs = juce::StringArray ("usb-1", "usb-2");
    p.midi.mapFile = "/maps/my map.json";
    p.library = { false, 15 };
    p.pluginFolder = "/opt/openmixer/plugins";
    return p;
}

struct MockTargets final : PreferencesTargets
{
    void applyPluginFolder (const juce::File& f) override { calls.add ("plugins:" + f.getFullPathName()); }
    bool applyTheme (const juce::String& id) override { calls.add ("theme:" + id); return true; }
    void applyAudio (const AudioPrefs& a) override { calls.add ("audio:" + juce::String (a.masterPair) + "," + juce::String (a.cuePair)); }
    void applyMidi (const MidiPrefs& m) override { calls.add ("midi:" + juce::String (m.enableAll ? "all" : m.enabledInputs.joinIntoString ("+"))); }
    juce::StringArray calls;
};
}

TEST (PreferencesTest, DefaultsAreSensible)
{
    const Preferences p;
    EXPECT_EQ (p.theme, "openmixer.dark");
    EXPECT_TRUE (p.midi.enableAll);
    EXPECT_EQ (p.audio.masterPair, 0);
    EXPECT_EQ (p.audio.cuePair, 1);
    EXPECT_TRUE (p.library.scanOnStart);
    EXPECT_EQ (p.library.scanIntervalMinutes, 5);
    EXPECT_EQ (p.effectivePluginFolder(), Preferences::defaultPluginFolder());
    EXPECT_EQ (p.effectiveMapFile(), Preferences::defaultMapFile());
}

TEST (PreferencesTest, JsonRoundTripsEveryField)
{
    const auto original = customised();
    Preferences loaded;
    ASSERT_TRUE (Preferences::fromJson (original.toJson(), loaded).wasOk());
    EXPECT_TRUE (loaded == original);
    EXPECT_EQ (loaded.effectivePluginFolder().getFullPathName(), "/opt/openmixer/plugins");
    EXPECT_EQ (loaded.toJson(), original.toJson());
}

TEST (PreferencesTest, JsonCarriesTheSchemaVersion)
{
    juce::var root;
    ASSERT_TRUE (juce::JSON::parse (Preferences().toJson(), root).wasOk());
    EXPECT_EQ (static_cast<int> (root.getProperty ("version", 0)), Preferences::kVersion);
}

TEST (PreferencesTest, MissingSectionsAndBadFieldsFallBackOneByOne)
{
    Preferences p;
    ASSERT_TRUE (Preferences::fromJson ("{\"version\":1,\"theme\":\"x\",\"audio\":{\"masterPair\":\"two\",\"cuePair\":99999},"
                                        "\"midi\":{\"enableAll\":\"yes\",\"enabledInputs\":[\"a\",3,\"a\",\"b\"]},"
                                        "\"library\":{\"scanIntervalMinutes\":0}}", p).wasOk());
    EXPECT_EQ (p.theme, "x");
    EXPECT_EQ (p.audio.masterPair, 0) << "wrong type keeps the default";
    EXPECT_EQ (p.audio.cuePair, 1) << "out of range keeps the default";
    EXPECT_TRUE (p.midi.enableAll);
    EXPECT_EQ (p.midi.enabledInputs.joinIntoString (","), "a,b") << "non-strings and duplicates dropped";
    EXPECT_EQ (p.library.scanIntervalMinutes, 5);
    EXPECT_EQ (p.pluginFolder, juce::String());
}

TEST (PreferencesTest, RejectsNonJsonAndUnsupportedVersions)
{
    Preferences p = customised();
    const auto before = p;
    for (const char* bad : { "", "not json", "[]", "{}", "{\"version\":0}", "{\"version\":2}", "{\"version\":\"1\"}" })
        EXPECT_TRUE (Preferences::fromJson (bad, p).failed()) << bad;
    EXPECT_TRUE (p == before) << "a refused file leaves the target untouched";
}

TEST (PreferencesTest, SaveAndLoadThroughAFile)
{
    openmixer::library::test::TempDir dir;
    const auto file = dir.dir.getChildFile ("nested/preferences.json");
    ASSERT_TRUE (customised().save (file).wasOk());   // creates the folder
    Preferences loaded;
    ASSERT_TRUE (Preferences::load (file, loaded).wasOk());
    EXPECT_TRUE (loaded == customised());
}

TEST (PreferencesTest, MissingFileMeansDefaults)
{
    openmixer::library::test::TempDir dir;
    Preferences loaded = customised();
    EXPECT_TRUE (Preferences::load (dir.dir.getChildFile ("none.json"), loaded).wasOk());
    EXPECT_TRUE (loaded == Preferences());
}

TEST (PreferencesTest, CorruptFileIsKeptAsBadAndDefaultsAreUsed)
{
    openmixer::library::test::TempDir dir;
    const auto file = dir.dir.getChildFile ("preferences.json");
    file.replaceWithText ("{ broken");
    Preferences loaded = customised();
    EXPECT_TRUE (Preferences::load (file, loaded).failed());
    EXPECT_TRUE (loaded == Preferences());
    EXPECT_EQ (dir.dir.getChildFile ("preferences.json.bad").loadFileAsString(), "{ broken");
    EXPECT_TRUE (Preferences().save (file).wasOk()) << "the next save replaces the broken file";
}

TEST (PreferencesTest, DefaultFileHonoursTheEnvironmentOverride)
{
    setenv ("OPENMIXER_PREFS", "/tmp/om_prefs_env/p.json", 1);
    EXPECT_EQ (Preferences::defaultFile().getFullPathName(), "/tmp/om_prefs_env/p.json");
    unsetenv ("OPENMIXER_PREFS");
    EXPECT_TRUE (Preferences::defaultFile().getFullPathName().endsWith ("OpenMixer/preferences.json"));
}

TEST (PreferencesApplierTest, FirstApplyPushesEverySectionInDependencyOrder)
{
    MockTargets targets;
    PreferencesApplier applier (targets);
    EXPECT_EQ (applier.apply (customised()), 4);
    ASSERT_EQ (targets.calls.size(), 4);
    EXPECT_TRUE (targets.calls[0].startsWith ("plugins:/opt/openmixer/plugins"));   // plugins can provide the theme
    EXPECT_EQ (targets.calls[1], "theme:openmixer.daylight");
    EXPECT_EQ (targets.calls[2], "audio:2,-1");
    EXPECT_EQ (targets.calls[3], "midi:usb-1+usb-2");
}

TEST (PreferencesApplierTest, UnchangedPreferencesPushNothing)
{
    MockTargets targets;
    PreferencesApplier applier (targets);
    applier.apply (customised());
    targets.calls.clear();
    EXPECT_EQ (applier.apply (customised()), 0);
    EXPECT_EQ (targets.calls.size(), 0);
}

TEST (PreferencesApplierTest, OnlyTheChangedSectionIsPushed)
{
    MockTargets targets;
    PreferencesApplier applier (targets);
    auto p = customised();
    applier.apply (p);

    targets.calls.clear();
    p.audio.cuePair = 1;
    EXPECT_EQ (applier.apply (p), 1);
    EXPECT_EQ (targets.calls.joinIntoString (";"), "audio:2,1");

    targets.calls.clear();
    p.midi.enableAll = true;
    applier.apply (p);
    EXPECT_EQ (targets.calls.joinIntoString (";"), "midi:all");

    targets.calls.clear();
    p.theme = "openmixer.dark";
    p.library.scanIntervalMinutes = 60;   // read at library start, nothing to push
    applier.apply (p);
    EXPECT_EQ (targets.calls.joinIntoString (";"), "theme:openmixer.dark");
}

TEST (PreferencesApplierTest, ChangingThePluginFolderReselectsTheTheme)
{
    MockTargets targets;
    PreferencesApplier applier (targets);
    auto p = customised();
    applier.apply (p);
    targets.calls.clear();
    p.pluginFolder = "/elsewhere";
    EXPECT_EQ (applier.apply (p), 2);
    EXPECT_EQ (targets.calls.joinIntoString (";"), "plugins:/elsewhere;theme:openmixer.daylight");
}

TEST (PreferencesApplierTest, EmptyFolderMeansTheDefaultFolderAndDoesNotRepeat)
{
    MockTargets targets;
    PreferencesApplier applier (targets);
    Preferences p;
    applier.apply (p);
    EXPECT_EQ (targets.calls[0], "plugins:" + Preferences::defaultPluginFolder().getFullPathName());
    targets.calls.clear();
    p.pluginFolder = Preferences::defaultPluginFolder().getFullPathName();   // same folder, spelled out
    EXPECT_EQ (applier.apply (p), 0);
}
