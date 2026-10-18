// OpenMixer LibraryImportTest - iTunes / Rekordbox importers, merge policy, transactionality
#include <JuceHeader.h>
#include "library/FileUrl.h"
#include "library/ImportCues.h"
#include "library/ImportMerge.h"
#include "library/ItunesImporter.h"
#include "library/LibraryImport.h"
#include "library/RekordboxImporter.h"

#include <gtest/gtest.h>

using namespace openmixer::library;

namespace
{
juce::File fixture (const char* name)
{
    return juce::File (OPENMIXER_TEST_DATA_DIR).getChildFile ("import").getChildFile (name);
}

std::unique_ptr<Library> memoryLibrary()
{
    juce::String error;
    auto lib = Library::open (":memory:", Database::OpenMode::ReadWrite, coreMigrations(), &error);
    EXPECT_NE (lib, nullptr) << error;
    return lib;
}

ImportedLibrary parseWith (const LibraryImporter& importer, const char* name, ImportReport& report)
{
    juce::XmlDocument doc (fixture (name));
    const auto root = doc.getDocumentElement();
    EXPECT_NE (root, nullptr) << doc.getLastParseError();
    ImportedLibrary data;
    if (root != nullptr)
    {
        EXPECT_TRUE (importer.recognises (*root));
        EXPECT_TRUE (importer.parse (*root, data, report).wasOk());
    }
    return data;
}

juce::Result runImport (Library& lib, const char* name, ImportReport& report)
{
    return importLibraryFile (lib, LibraryImporterRegistry::withDefaults(), fixture (name), report);
}

std::optional<Playlist> childNamed (Library& lib, std::optional<std::int64_t> parent, const juce::String& name)
{
    for (const auto& p : lib.playlists().children (parent))
        if (p.name == name)
            return p;
    return std::nullopt;
}

std::vector<juce::String> entryTitles (Library& lib, std::int64_t playlistId)
{
    std::vector<juce::String> titles;
    for (const auto& e : lib.playlists().entries (playlistId))
        titles.push_back (lib.tracks().findById (e.trackId)->title);
    return titles;
}

const juce::String kPulseOne = "/music/Techno/Pulse One.flac";
} // namespace

TEST (ImportFileUrl, DecodesCommonForms)
{
    EXPECT_EQ (fileUrlToPath ("file:///music/a%20b.mp3"), "/music/a b.mp3");
    EXPECT_EQ (fileUrlToPath ("file://localhost/music/x.wav"), "/music/x.wav");
    EXPECT_EQ (fileUrlToPath ("FILE://localhost/music/x.wav"), "/music/x.wav");
    EXPECT_EQ (fileUrlToPath ("file:///music/Caf%C3%A9+Mix%25.wav"), juce::String (juce::CharPointer_UTF8 ("/music/Caf\xc3\xa9+Mix%.wav")));
}

TEST (ImportFileUrl, WindowsDriveLettersAndUnc)
{
#if ! JUCE_WINDOWS
    EXPECT_EQ (fileUrlToPath ("file://localhost/C:/Users/dj/a%20b.wav"), "C:/Users/dj/a b.wav");
    EXPECT_EQ (fileUrlToPath ("file:///D:/m.wav"), "D:/m.wav");
    EXPECT_EQ (fileUrlToPath ("file://nas/share/m.wav"), "//nas/share/m.wav");
#endif
}

TEST (ImportFileUrl, RejectsNonFileAndBrokenEscapes)
{
    EXPECT_TRUE (fileUrlToPath ("http://example.invalid/a").isEmpty());
    EXPECT_TRUE (fileUrlToPath ("").isEmpty());
    EXPECT_TRUE (fileUrlToPath ("file://").isEmpty());
    EXPECT_EQ (fileUrlToPath ("file:///a%2"), "/a%2");        // truncated escape stays literal
    EXPECT_EQ (fileUrlToPath ("file:///a%zz"), "/a%zz");
}

TEST (ImportItunesParse, TracksMetadataAndSkips)
{
    ImportReport report;
    const auto data = parseWith (ItunesImporter(), "itunes_small.xml", report);
    ASSERT_EQ (data.tracks.size(), 2u);
    EXPECT_EQ (report.tracksSkipped, 2);                       // URL track and the one without a location
    const auto& t = data.tracks[0].record;
    EXPECT_EQ (t.path, "/music/House/Night Drive & Dawn+Mix.mp3");
    EXPECT_EQ (t.title, "Night Drive & Dawn");
    EXPECT_EQ (t.rating, 4);
    EXPECT_EQ (t.playCount, 7);
    EXPECT_EQ (t.year, 2019);
    EXPECT_DOUBLE_EQ (t.durationSec, 215.5);
    EXPECT_EQ (t.bpm, std::optional<double> (124.0));
    EXPECT_GT (t.dateAddedMs, 0);
    EXPECT_TRUE (t.lastPlayedMs.has_value());
    EXPECT_EQ (data.tracks[1].record.rating, 0);               // computed rating is not user intent
    EXPECT_EQ (data.tracks[1].record.path, juce::String (juce::CharPointer_UTF8 ("/music/Caf\xc3\xa9 Loop.wav")));
}

TEST (ImportItunesParse, PlaylistTreeSkipsMasterAndKeepsFolders)
{
    ImportReport report;
    const auto data = parseWith (ItunesImporter(), "itunes_small.xml", report);
    ASSERT_EQ (data.playlists.size(), 2u);
    EXPECT_EQ (data.playlists[0].name, "Sets");
    EXPECT_TRUE (data.playlists[0].isFolder);
    ASSERT_EQ (data.playlists[0].children.size(), 1u);
    EXPECT_EQ (data.playlists[0].children[0].name, "Warm Up");
    EXPECT_EQ (data.playlists[0].children[0].trackKeys.size(), 3u);
    EXPECT_EQ (data.playlists[1].name, "Favourites");
}

TEST (ImportItunesParse, RecognisesOnlyItsOwnFormat)
{
    juce::XmlDocument rb (fixture ("rekordbox_small.xml"));
    juce::XmlDocument it (fixture ("itunes_small.xml"));
    EXPECT_FALSE (ItunesImporter().recognises (*rb.getDocumentElement()));
    EXPECT_FALSE (RekordboxImporter().recognises (*it.getDocumentElement()));
}

TEST (ImportRekordboxParse, TrackFieldsTempoAndMarks)
{
    ImportReport report;
    const auto data = parseWith (RekordboxImporter(), "rekordbox_small.xml", report);
    ASSERT_EQ (data.tracks.size(), 2u);
    EXPECT_EQ (report.tracksSkipped, 1);
    const auto& t = data.tracks[0];
    EXPECT_EQ (t.record.path, kPulseOne);
    EXPECT_EQ (t.record.rating, 3);
    EXPECT_EQ (t.record.key, "Am");
    EXPECT_EQ (t.record.bpm, std::optional<double> (128.0));
    EXPECT_DOUBLE_EQ (t.sampleRate, 48000.0);
    EXPECT_GT (t.record.dateAddedMs, 0);
    EXPECT_EQ (t.cues.size(), 5u);                             // the fade mark is dropped
    EXPECT_EQ (t.cues[0].colourArgb, 0xff28e214u);
    EXPECT_EQ (data.tracks[1].record.bpm, std::optional<double> (130.5));   // falls back to TEMPO
    EXPECT_EQ (data.tracks[1].record.rating, 5);
}

TEST (ImportRekordboxParse, PlaylistTreeAndLocationKeys)
{
    ImportReport report;
    const auto data = parseWith (RekordboxImporter(), "rekordbox_small.xml", report);
    ASSERT_EQ (data.playlists.size(), 2u);                     // ROOT itself is not imported
    EXPECT_TRUE (data.playlists[0].isFolder);
    ASSERT_EQ (data.playlists[0].children.size(), 1u);
    EXPECT_EQ (data.playlists[0].children[0].trackKeys.size(), 3u);
    ASSERT_EQ (data.playlists[1].trackKeys.size(), 1u);
    EXPECT_EQ (data.playlists[1].trackKeys[0], "1");           // KeyType 1 resolved through Location
}

TEST (ImportItunes, WritesTracksPlaylistsAndReport)
{
    auto lib = memoryLibrary();
    ImportReport report;
    ASSERT_TRUE (runImport (*lib, "itunes_small.xml", report).wasOk());
    EXPECT_EQ (report.tracksAdded, 2);
    EXPECT_EQ (report.tracksMerged, 0);
    EXPECT_EQ (report.tracksSkipped, 2);
    EXPECT_EQ (report.playlistsCreated, 3);
    EXPECT_EQ (report.entriesAdded, 3);
    EXPECT_EQ (report.entriesSkipped, 1);                      // the remote track in "Warm Up"
    EXPECT_EQ (report.errors, 0);
    EXPECT_EQ (lib->tracks().count(), 2);

    const auto sets = childNamed (*lib, std::nullopt, "Sets");
    ASSERT_TRUE (sets && sets->isFolder);
    const auto warm = childNamed (*lib, sets->id, "Warm Up");
    ASSERT_TRUE (warm.has_value());
    EXPECT_EQ (entryTitles (*lib, warm->id), (std::vector<juce::String> { juce::String (juce::CharPointer_UTF8 ("Caf\xc3\xa9 Loop")), "Night Drive & Dawn" }));
}

TEST (ImportRekordbox, CuesAreStoredInFramesAtTheTrackSampleRate)
{
    auto lib = memoryLibrary();
    ImportReport report;
    ASSERT_TRUE (runImport (*lib, "rekordbox_small.xml", report).wasOk());
    EXPECT_EQ (report.tracksAdded, 2);
    EXPECT_EQ (report.errors, 1);                              // loop with end before start
    EXPECT_EQ (report.cuesImported, 5);

    const auto id = lib->tracks().findByPath (kPulseOne)->id;
    const auto cues = lib->cues().cues (id);
    ASSERT_EQ (cues.size(), 3u);
    EXPECT_EQ (cues[0].slot, 0);
    EXPECT_EQ (cues[0].positionFrames, 48000);
    EXPECT_EQ (cues[0].label, "Intro");
    EXPECT_EQ (cues[1].positionFrames, 1560000);               // 32.5 s * 48000
    EXPECT_EQ (cues[2].slot, kFirstMemorySlot);                // memory cue
    EXPECT_EQ (cues[2].positionFrames, 64 * 48000);
    const auto loops = lib->cues().loops (id);
    ASSERT_EQ (loops.size(), 2u);
    EXPECT_EQ (loops[0].slot, 2);
    EXPECT_EQ (loops[0].endFrames, 100 * 48000);
    EXPECT_EQ (loops[1].slot, kFirstMemorySlot);
}

TEST (ImportRekordbox, UnknownSampleRateAssumes44100)
{
    auto lib = memoryLibrary();
    ImportReport report;
    ASSERT_TRUE (runImport (*lib, "rekordbox_small.xml", report).wasOk());
    const auto two = lib->tracks().findByPath ("C:/Users/dj/Music/Pulse Two.wav");
#if ! JUCE_WINDOWS
    ASSERT_TRUE (two.has_value());
    EXPECT_EQ (two->title, "Pulse Two");
#endif
    ImportedTrack t;
    t.record.path = "/x";
    t.cues.push_back ({ ImportedCue::Kind::Cue, 0, 2.0, 0.0, "", 0 });
    const auto id = lib->tracks().insert (t.record);
    ImportReport r;
    ASSERT_TRUE (importCues (lib->cues(), *id, t, r).wasOk());
    EXPECT_EQ (lib->cues().cues (*id)[0].positionFrames, 88200);
}

TEST (ImportRekordbox, PlaylistsFolderAndMissingTrackEntry)
{
    auto lib = memoryLibrary();
    ImportReport report;
    ASSERT_TRUE (runImport (*lib, "rekordbox_small.xml", report).wasOk());
    EXPECT_EQ (report.playlistsCreated, 3);
    EXPECT_EQ (report.entriesAdded, 3);                        // peak time: 2 (track 3 skipped), by location: 1
    EXPECT_EQ (report.entriesSkipped, 1);
    const auto club = childNamed (*lib, std::nullopt, "Club");
    ASSERT_TRUE (club.has_value());
    const auto peak = childNamed (*lib, club->id, "Peak Time");
    ASSERT_TRUE (peak.has_value());
    EXPECT_EQ (entryTitles (*lib, peak->id), (std::vector<juce::String> { "Pulse Two", "Pulse One" }));
}

TEST (ImportDedupe, ReimportAddsNothing)
{
    auto lib = memoryLibrary();
    ImportReport first, second;
    ASSERT_TRUE (runImport (*lib, "rekordbox_small.xml", first).wasOk());
    ASSERT_TRUE (runImport (*lib, "rekordbox_small.xml", second).wasOk());
    EXPECT_EQ (second.tracksAdded, 0);
    EXPECT_EQ (second.tracksMerged, 2);
    EXPECT_EQ (second.cuesImported, 0);
    EXPECT_EQ (second.playlistsCreated, 0);
    EXPECT_EQ (second.entriesAdded, 0);
    EXPECT_EQ (lib->tracks().count(), 2);
    EXPECT_EQ (lib->cues().cues (lib->tracks().findByPath (kPulseOne)->id).size(), 3u);
}

TEST (ImportMergePolicy, ImportFillsGapsAndLocalDataWins)
{
    auto lib = memoryLibrary();
    TrackRecord local;
    local.path = kPulseOne;
    local.contentHash = "abc";
    local.title = "My Title";                                 // set locally: kept
    local.rating = 0;                                          // default: import may fill
    local.playCount = 9;                                       // set locally: kept
    local.dateAddedMs = 123;
    const auto id = lib->tracks().insert (local);
    ASSERT_TRUE (id.has_value());
    ASSERT_TRUE (lib->cues().setCue ({ *id, 0, 777, 0, "mine" }).wasOk());   // slot 0 taken locally

    ImportReport report;
    ASSERT_TRUE (runImport (*lib, "rekordbox_small.xml", report).wasOk());
    EXPECT_EQ (report.tracksMerged, 1);
    EXPECT_EQ (report.tracksAdded, 1);

    const auto merged = lib->tracks().findById (*id);
    EXPECT_EQ (merged->title, "My Title");
    EXPECT_EQ (merged->artist, "Fixture Artist");              // gap filled
    EXPECT_EQ (merged->rating, 3);                             // default rating filled
    EXPECT_EQ (merged->playCount, 9);                          // local count kept
    EXPECT_EQ (merged->contentHash, "abc");
    EXPECT_EQ (merged->dateAddedMs, 123);
    EXPECT_EQ (merged->bpm, std::optional<double> (128.0));

    const auto cues = lib->cues().cues (*id);
    ASSERT_EQ (cues.size(), 3u);                               // slot 0 kept, slot 1 and memory added
    EXPECT_EQ (cues[0].label, "mine");
    EXPECT_EQ (cues[0].positionFrames, 777);
}

TEST (ImportMergePolicy, ExistingRatingIsNotOverwritten)
{
    TrackRecord local, imported;
    local.rating = 2;
    local.playCount = 0;
    imported.rating = 5;
    imported.playCount = 12;
    EXPECT_TRUE (mergeTrackRecord (local, imported));
    EXPECT_EQ (local.rating, 2);
    EXPECT_EQ (local.playCount, 12);
    EXPECT_FALSE (mergeTrackRecord (local, imported));
}

TEST (ImportTransaction, DatabaseErrorRollsEverythingBack)
{
    auto lib = memoryLibrary();
    ASSERT_TRUE (lib->database().exec ("DROP TABLE playlist_entries").wasOk());
    ImportReport report;
    const auto result = runImport (*lib, "itunes_small.xml", report);
    EXPECT_TRUE (result.failed());
    EXPECT_EQ (lib->tracks().count(), 0);                 // tracks inserted before the failure are gone
    EXPECT_TRUE (lib->playlists().children (std::nullopt).empty());
    EXPECT_EQ (report.tracksAdded, 0);
    EXPECT_GE (report.errors, 1);
}

TEST (ImportTransaction, FailedLookupIsNotTreatedAsNew)
{
    auto lib = memoryLibrary();
    ImportReport first;
    ASSERT_TRUE (runImport (*lib, "rekordbox_small.xml", first).wasOk());
    ASSERT_TRUE (lib->database().exec ("ALTER TABLE tracks RENAME TO tracks_away").wasOk());
    ImportReport second;
    EXPECT_TRUE (runImport (*lib, "rekordbox_small.xml", second).failed());
    ASSERT_TRUE (lib->database().exec ("ALTER TABLE tracks_away RENAME TO tracks").wasOk());
    EXPECT_EQ (lib->tracks().count(), 2);                      // no duplicates
}

TEST (ImportRegistry, UnrecognisedAndUnreadableFilesFail)
{
    auto lib = memoryLibrary();
    ImportReport report;
    const auto dir = juce::File::getSpecialLocation (juce::File::tempDirectory).getChildFile ("om_import_test");
    dir.createDirectory();
    const auto other = dir.getChildFile ("other.xml");
    other.replaceWithText ("<something/>");
    const auto registry = LibraryImporterRegistry::withDefaults();
    EXPECT_TRUE (importLibraryFile (*lib, registry, other, report).failed());
    other.replaceWithText ("<not closed");
    EXPECT_TRUE (importLibraryFile (*lib, registry, other, report).failed());
    EXPECT_TRUE (importLibraryFile (*lib, registry, dir.getChildFile ("missing.xml"), report).failed());
    EXPECT_EQ (lib->tracks().count(), 0);
    dir.deleteRecursively();
}

TEST (ImportRegistry, LaterRegistrationWinsAndNamesListed)
{
    struct Fake : LibraryImporter
    {
        juce::String name() const override { return "fake"; }
        bool recognises (const juce::XmlElement& r) const override { return r.hasTagName ("DJ_PLAYLISTS"); }
        juce::Result parse (const juce::XmlElement&, ImportedLibrary& out, ImportReport&) const override
        {
            out.tracks.push_back ({});
            out.tracks.back().record.path = "/fake";
            return juce::Result::ok();
        }
    };
    auto registry = LibraryImporterRegistry::withDefaults();
    EXPECT_EQ (registry.names().size(), 2);
    registry.add (std::make_shared<Fake>());
    auto lib = memoryLibrary();
    ImportReport report;
    ASSERT_TRUE (importLibraryFile (*lib, registry, fixture ("rekordbox_small.xml"), report).wasOk());
    EXPECT_TRUE (lib->tracks().findByPath ("/fake").has_value());
    EXPECT_EQ (lib->tracks().count(), 1);
}

TEST (ImportCuesOrder, HotcueWinsOverEarlierMemoryCueAtSamePosition)
{
    auto lib = memoryLibrary();
    ImportedTrack t;
    t.record.path = "/x";
    t.cues.push_back ({ ImportedCue::Kind::Cue, -1, 5.0, 0.0, "memory", 0 });
    t.cues.push_back ({ ImportedCue::Kind::Cue, 3, 5.0, 0.0, "hot", 0 });
    const auto id = lib->tracks().insert (t.record);
    ImportReport r;
    ASSERT_TRUE (importCues (lib->cues(), *id, t, r).wasOk());
    const auto cues = lib->cues().cues (*id);
    ASSERT_EQ (cues.size(), 1u);
    EXPECT_EQ (cues[0].slot, 3);
    EXPECT_EQ (cues[0].label, "hot");
}

TEST (ImportMergePolicy, PlaylistsMergeIntoSameNamedSibling)
{
    auto lib = memoryLibrary();
    TrackRecord mine;
    mine.path = "/mine.wav";
    mine.title = "Mine";
    const auto mineId = lib->tracks().insert (mine);
    const auto fav = lib->playlists().create ("Favourites");
    ASSERT_TRUE (fav.has_value());
    ASSERT_TRUE (lib->playlists().addTrack (*fav, *mineId).has_value());
    ImportReport report;
    ASSERT_TRUE (runImport (*lib, "itunes_small.xml", report).wasOk());
    EXPECT_EQ (report.playlistsCreated, 2);                    // Sets + Warm Up; Favourites reused
    EXPECT_EQ (lib->playlists().children (std::nullopt).size(), 2u);
    EXPECT_EQ (entryTitles (*lib, *fav), (std::vector<juce::String> { "Mine", "Night Drive & Dawn" }));
}
