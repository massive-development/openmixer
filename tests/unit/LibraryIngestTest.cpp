// OpenMixer LibraryIngestTest - metadata readers, folder ingest, file state, watch-folder list
#include <JuceHeader.h>
#include "LibraryIngestTestUtil.h"
#include "analysis/ClipSource.h"
#include "library/Ingestor.h"
#include "library/JuceMetadataReader.h"

#include <gtest/gtest.h>

using namespace openmixer::library;
using openmixer::library::test::TempDir;
using openmixer::library::test::writeWav;

namespace
{
std::unique_ptr<Library> memoryLibrary()
{
    juce::String error;
    auto lib = Library::open (":memory:", Database::OpenMode::ReadWrite, coreMigrations(), &error);
    EXPECT_NE (lib, nullptr) << error;
    return lib;
}

juce::StringPairArray tags()
{
    juce::StringPairArray t;
    t.set (juce::WavAudioFormat::riffInfoTitle, "Night Drive");
    t.set (juce::WavAudioFormat::riffInfoArtist, "The Artist");
    t.set (juce::WavAudioFormat::riffInfoProductName, "The Album");
    t.set (juce::WavAudioFormat::riffInfoGenre, "House");
    t.set (juce::WavAudioFormat::riffInfoDateCreated, "2019-05-01");
    t.set (juce::WavAudioFormat::riffInfoComment2, "nice");
    return t;
}

struct Fixture
{
    std::unique_ptr<Library> lib = memoryLibrary();
    MetadataReaderRegistry readers = MetadataReaderRegistry::withDefaults();
    Ingestor ingestor { *lib, readers };
    TempDir tmp;

    IngestSummary scan (const juce::File& folder, juce::Result* result = nullptr)
    {
        IngestSummary s;
        auto r = ingestor.ingestFolder (folder, s);
        if (result != nullptr)
            *result = r;
        return s;
    }
};

juce::File named (const char* name)
{
    return juce::File::getSpecialLocation (juce::File::tempDirectory).getChildFile (name);
}

void touch (const juce::File& f, juce::int64 ms) { f.setLastModificationTime (juce::Time (ms)); }
} // namespace

TEST (MetadataReaderTest, JuceReaderReadsRiffInfoAndDuration)
{
    TempDir tmp;
    const auto f = writeWav (tmp.dir.getChildFile ("a.wav"), 2.0, tags());
    TrackMetadata m;
    ASSERT_TRUE (JuceMetadataReader().read (f, m).wasOk());
    EXPECT_EQ (m.title, "Night Drive");
    EXPECT_EQ (m.artist, "The Artist");
    EXPECT_EQ (m.album, "The Album");
    EXPECT_EQ (m.genre, "House");
    EXPECT_EQ (m.year, 2019);
    EXPECT_EQ (m.comment, "nice");
    EXPECT_NEAR (m.durationSec, 2.0, 0.01);
}

TEST (MetadataReaderTest, UntaggedFileFallsBackToFileNameTitle)
{
    TempDir tmp;
    const auto f = writeWav (tmp.dir.getChildFile ("Some Song.wav"), 0.5);
    TrackMetadata m;
    ASSERT_TRUE (JuceMetadataReader().read (f, m).wasOk());
    EXPECT_EQ (m.title, "Some Song");
    EXPECT_TRUE (m.artist.isEmpty());
}

TEST (MetadataReaderTest, CorruptFileFails)
{
    TempDir tmp;
    const auto f = tmp.dir.getChildFile ("bad.wav");
    f.replaceWithText ("this is not audio");
    TrackMetadata m;
    EXPECT_TRUE (JuceMetadataReader().read (f, m).failed());
}

namespace
{
class FakeReader final : public MetadataReader
{
public:
    juce::String name() const override { return "fake"; }
    juce::StringArray extensions() const override { return { "wav", "xyz" }; }
    juce::Result read (const juce::File&, TrackMetadata& out) override { out.title = "fake"; return juce::Result::ok(); }
};
} // namespace

TEST (MetadataReaderTest, RegistryLaterReaderWinsAndListsExtensions)
{
    auto registry = MetadataReaderRegistry::withDefaults();
    EXPECT_TRUE (registry.supportedExtensions().contains ("wav"));
    EXPECT_FALSE (registry.supports (named ("song.xyz")));
    EXPECT_FALSE (registry.supports (named ("notes.txt")));

    registry.add (std::make_shared<FakeReader>());
    EXPECT_EQ (registry.find (named ("a.WAV"))->name(), "fake");   // extension match is case-insensitive
    EXPECT_TRUE (registry.supports (named ("song.xyz")));
}

TEST (IngestTest, AddsTracksRecursivelyWithTagsAndHash)
{
    Fixture f;
    const auto a = writeWav (f.tmp.dir.getChildFile ("a.wav"), 1.0, tags());
    writeWav (f.tmp.dir.getChildFile ("sub/deeper/b.wav"), 1.0, {}, 880.0f);
    f.tmp.dir.getChildFile ("readme.txt").replaceWithText ("ignore me");

    const auto s = f.scan (f.tmp.dir);
    EXPECT_EQ (s.scanned, 2);
    EXPECT_EQ (s.added, 2);
    EXPECT_EQ (f.lib->tracks().count(), 2);

    const auto t = f.lib->tracks().findByPath (a.getFullPathName());
    ASSERT_TRUE (t.has_value());
    EXPECT_EQ (t->title, "Night Drive");
    EXPECT_EQ (t->artist, "The Artist");
    EXPECT_EQ (t->contentHash, openmixer::analysis::hashFileContent (a));
    EXPECT_GT (t->dateAddedMs, 0);

    const auto state = f.lib->trackFiles().find (t->id);
    ASSERT_TRUE (state.has_value());
    EXPECT_EQ (state->sizeBytes, a.getSize());
    EXPECT_FALSE (state->missing);
}

TEST (IngestTest, RescanSkipsUnchangedFiles)
{
    Fixture f;
    writeWav (f.tmp.dir.getChildFile ("a.wav"), 1.0);
    f.scan (f.tmp.dir);
    const auto s = f.scan (f.tmp.dir);
    EXPECT_EQ (s.unchanged, 1);
    EXPECT_EQ (s.added + s.updated, 0);
}

TEST (IngestTest, ChangedFileIsReReadAndKeepsUserData)
{
    Fixture f;
    const auto file = writeWav (f.tmp.dir.getChildFile ("a.wav"), 1.0);
    touch (file, 1'600'000'000'000);
    f.scan (f.tmp.dir);
    const auto id = f.lib->tracks().findByPath (file.getFullPathName())->id;
    f.lib->tracks().setRating (id, 5);

    writeWav (file, 2.0, tags());
    touch (file, 1'700'000'000'000);
    const auto s = f.scan (f.tmp.dir);
    EXPECT_EQ (s.updated, 1);

    const auto t = f.lib->tracks().findById (id);
    ASSERT_TRUE (t.has_value());
    EXPECT_EQ (t->title, "Night Drive");
    EXPECT_NEAR (t->durationSec, 2.0, 0.01);
    EXPECT_EQ (t->rating, 5);
    EXPECT_EQ (f.lib->tracks().count(), 1);
}

TEST (IngestTest, SameSizeDifferentMtimeIsReRead)
{
    Fixture f;
    const auto file = writeWav (f.tmp.dir.getChildFile ("a.wav"), 1.0);
    touch (file, 1'600'000'000'000);
    f.scan (f.tmp.dir);
    touch (file, 1'600'000'005'000);
    EXPECT_EQ (f.scan (f.tmp.dir).updated, 1);
}

TEST (IngestTest, VanishedFileIsMarkedMissingAndRestoredWhenItReturns)
{
    Fixture f;
    const auto keep = writeWav (f.tmp.dir.getChildFile ("keep.wav"), 1.0);
    const auto gone = writeWav (f.tmp.dir.getChildFile ("gone.wav"), 1.0, {}, 300.0f);
    f.scan (f.tmp.dir);
    const auto id = f.lib->tracks().findByPath (gone.getFullPathName())->id;
    f.lib->tracks().setRating (id, 4);

    const auto backup = f.tmp.dir.getSiblingFile ("om_backup.wav");
    ASSERT_TRUE (gone.moveFileTo (backup));
    auto s = f.scan (f.tmp.dir);
    EXPECT_EQ (s.missing, 1);
    EXPECT_TRUE (f.lib->trackFiles().find (id)->missing);
    EXPECT_FALSE (f.lib->trackFiles().find (f.lib->tracks().findByPath (keep.getFullPathName())->id)->missing);
    EXPECT_EQ (f.lib->tracks().count(), 1 + 1);   // row kept, not deleted

    s = f.scan (f.tmp.dir);
    EXPECT_EQ (s.missing, 0);   // already flagged: not counted twice

    ASSERT_TRUE (backup.moveFileTo (gone));
    s = f.scan (f.tmp.dir);
    EXPECT_EQ (s.restored, 1);
    EXPECT_FALSE (f.lib->trackFiles().find (id)->missing);
    EXPECT_EQ (f.lib->tracks().findById (id)->rating, 4);
    EXPECT_EQ (f.lib->trackFiles().missingCount(), 0);
}

TEST (IngestTest, MissingFolderFailsAndFlagsNothing)
{
    Fixture f;
    writeWav (f.tmp.dir.getChildFile ("a.wav"), 1.0);
    f.scan (f.tmp.dir);

    juce::Result r = juce::Result::ok();
    const auto gone = f.tmp.dir.getChildFile ("nope");
    f.scan (gone, &r);
    EXPECT_TRUE (r.failed());
    EXPECT_EQ (f.lib->trackFiles().missingCount(), 0);
}

TEST (IngestTest, OnlyFlagsTracksUnderTheScannedFolder)
{
    Fixture f;
    TempDir other;
    writeWav (f.tmp.dir.getChildFile ("a.wav"), 1.0);
    const auto o = writeWav (other.dir.getChildFile ("o.wav"), 1.0, {}, 500.0f);
    f.scan (f.tmp.dir);
    f.scan (other.dir);
    o.deleteFile();
    f.scan (f.tmp.dir);   // other folder's file is gone but this scan does not cover it
    EXPECT_EQ (f.lib->trackFiles().missingCount(), 0);
}

TEST (IngestTest, CorruptFileCountsAsFailedAndDoesNotFlagExistingRowMissing)
{
    Fixture f;
    const auto file = writeWav (f.tmp.dir.getChildFile ("a.wav"), 1.0);
    touch (file, 1'600'000'000'000);
    f.scan (f.tmp.dir);

    file.replaceWithText ("garbage");
    touch (file, 1'700'000'000'000);
    const auto s = f.scan (f.tmp.dir);
    EXPECT_EQ (s.failed, 1);
    EXPECT_EQ (s.missing, 0);
    EXPECT_EQ (f.lib->tracks().count(), 1);
}

TEST (IngestTest, StopCheckAbortsWithoutFlaggingMissing)
{
    Fixture f;
    writeWav (f.tmp.dir.getChildFile ("a.wav"), 1.0);
    f.scan (f.tmp.dir);

    IngestSummary s;
    const auto r = f.ingestor.ingestFolder (f.tmp.dir, s, [] { return true; });
    EXPECT_TRUE (r.failed());
    EXPECT_EQ (f.lib->trackFiles().missingCount(), 0);
}

TEST (IngestTest, FailedLookupIsNotTreatedAsNewFile)
{
    Fixture f;
    writeWav (f.tmp.dir.getChildFile ("a.wav"), 1.0);
    f.lib->database().exec ("DROP TABLE track_files;");   // lookups of file state now fail
    f.scan (f.tmp.dir);
    const auto s = f.scan (f.tmp.dir);
    EXPECT_EQ (s.failed, 1);
    EXPECT_EQ (s.added + s.updated, 0);
}

TEST (IngestTest, ContentHashIsStableAcrossRenames)
{
    Fixture f;
    const auto a = writeWav (f.tmp.dir.getChildFile ("a.wav"), 1.0);
    f.scan (f.tmp.dir);
    const auto hash = f.lib->tracks().findByPath (a.getFullPathName())->contentHash;
    const auto b = f.tmp.dir.getChildFile ("renamed.wav");
    a.moveFileTo (b);
    f.scan (f.tmp.dir);
    EXPECT_EQ (f.lib->tracks().findByPath (b.getFullPathName())->contentHash, hash);
    EXPECT_EQ (f.lib->tracks().findByContentHash (hash).size(), 2u);   // old row stays, flagged missing
}

TEST (LibraryMigrationV2Test, UpgradesAV1DatabaseWithoutTouchingTracks)
{
    juce::String error;
    auto db = Database::open (":memory:", Database::OpenMode::ReadWrite, &error);
    ASSERT_NE (db, nullptr) << error;
    const std::vector<Migration> v1Only { coreMigrations().front() };
    ASSERT_TRUE (migrate (*db, v1Only).wasOk());
    TrackDao (*db).insert ([] { TrackRecord t; t.path = "/x.wav"; return t; }());

    ASSERT_TRUE (migrate (*db, coreMigrations()).wasOk());
    EXPECT_EQ (currentSchemaVersion (*db), 2);
    EXPECT_EQ (TrackDao (*db).count(), 1);
    EXPECT_TRUE (WatchFolderDao (*db).add ("/music").wasOk());
}

TEST (WatchFolderDaoTest, AddIsIdempotentAndRemoveWorks)
{
    auto lib = memoryLibrary();
    auto& dao = lib->watchFolders();
    EXPECT_TRUE (dao.add ("/music").wasOk());
    EXPECT_TRUE (dao.add ("/music").wasOk());
    EXPECT_TRUE (dao.add ("/more").wasOk());
    juce::Result r = juce::Result::ok();
    EXPECT_EQ (dao.list (&r).size(), 2u);
    EXPECT_TRUE (r.wasOk());
    EXPECT_TRUE (dao.remove ("/music").wasOk());
    ASSERT_EQ (dao.list().size(), 1u);
    EXPECT_EQ (dao.list().front(), "/more");
}

TEST (WatchFolderDaoTest, DeletingATrackCascadesItsFileState)
{
    auto lib = memoryLibrary();
    TrackRecord t;
    t.path = "/x.wav";
    const auto id = *lib->tracks().insert (t);
    lib->trackFiles().set (id, 1, 2);
    lib->tracks().remove (id);
    EXPECT_FALSE (lib->trackFiles().find (id).has_value());
}
