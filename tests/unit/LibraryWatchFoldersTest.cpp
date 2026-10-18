// OpenMixer LibraryWatchFoldersTest - persisted folder list and background rescans
#include <JuceHeader.h>
#include "LibraryIngestTestUtil.h"
#include "library/WatchFolders.h"

#include <gtest/gtest.h>

using namespace openmixer::library;
using openmixer::library::test::TempDir;
using openmixer::library::test::writeWav;

namespace
{
constexpr int kWaitMs = 10000;

struct Fixture
{
    TempDir tmp;
    juce::File dbFile = tmp.dir.getChildFile ("library.db");
    juce::File music = tmp.dir.getChildFile ("music");
    std::atomic<std::int64_t> fakeNow { 0 };

    Fixture() { music.createDirectory(); }

    std::unique_ptr<WatchFolders> make (std::int64_t intervalMs = 1000, bool scanOnStart = false)
    {
        WatchFolders::Options o;
        o.intervalMs = intervalMs;
        o.pollMs = 20;
        o.scanOnStart = scanOnStart;
        o.clock = [this] { return fakeNow.load(); };
        juce::String error;
        auto w = WatchFolders::create (dbFile.getFullPathName(), MetadataReaderRegistry::withDefaults(), o, &error);
        EXPECT_NE (w, nullptr) << error;
        return w;
    }

    int trackCount()
    {
        auto lib = Library::open (dbFile.getFullPathName(), Database::OpenMode::ReadOnly);
        return lib->tracks().count();
    }
};
} // namespace

TEST (WatchFoldersTest, FolderListPersistsAcrossInstances)
{
    Fixture f;
    {
        auto w = f.make();
        EXPECT_TRUE (w->add (f.music).wasOk());
        EXPECT_TRUE (w->add (f.music).wasOk());
        ASSERT_EQ (w->list().size(), 1u);
    }
    auto again = f.make();
    ASSERT_EQ (again->list().size(), 1u);
    EXPECT_EQ (again->list().front(), f.music.getFullPathName());
    EXPECT_TRUE (again->remove (f.music).wasOk());
    EXPECT_TRUE (again->list().empty());
}

TEST (WatchFoldersTest, AddRejectsNonFolder)
{
    Fixture f;
    auto w = f.make();
    EXPECT_TRUE (w->add (f.music.getChildFile ("missing")).failed());
    EXPECT_TRUE (w->list().empty());
}

TEST (WatchFoldersTest, AddingAFolderTriggersAScanOnTheBackgroundThread)
{
    Fixture f;
    writeWav (f.music.getChildFile ("a.wav"), 0.5);
    auto w = f.make();
    ASSERT_TRUE (w->add (f.music).wasOk());
    ASSERT_TRUE (w->waitForScans (1, kWaitMs));
    EXPECT_EQ (f.trackCount(), 1);
}

TEST (WatchFoldersTest, NothingScansUntilTheFakeClockReachesTheInterval)
{
    Fixture f;
    auto w = f.make (1000, false);
    w->add (f.music);
    ASSERT_TRUE (w->waitForScans (1, kWaitMs));   // the add itself triggers one pass

    writeWav (f.music.getChildFile ("late.wav"), 0.5);
    f.fakeNow = 999;
    w->poke();
    juce::Thread::sleep (150);   // several poll periods: still no pass
    EXPECT_EQ (w->scanCount(), 1);
    EXPECT_EQ (f.trackCount(), 0);

    f.fakeNow = 1000;
    w->poke();
    ASSERT_TRUE (w->waitForScans (2, kWaitMs));
    EXPECT_EQ (f.trackCount(), 1);
}

TEST (WatchFoldersTest, ManualTriggerScansImmediatelyAndReportsSummary)
{
    Fixture f;
    std::atomic<int> added { 0 };
    WatchFolders::Options o;
    o.intervalMs = 1'000'000;
    o.pollMs = 20;
    o.scanOnStart = false;
    o.clock = [&f] { return f.fakeNow.load(); };
    o.onScanDone = [&added] (const IngestSummary& s) { added += s.added; };
    auto w = WatchFolders::create (f.dbFile.getFullPathName(), MetadataReaderRegistry::withDefaults(), o);
    ASSERT_NE (w, nullptr);
    w->add (f.music);
    ASSERT_TRUE (w->waitForScans (1, kWaitMs));

    writeWav (f.music.getChildFile ("b.wav"), 0.5);
    w->triggerScan();
    ASSERT_TRUE (w->waitForScans (2, kWaitMs));
    EXPECT_EQ (added.load(), 1);
}

TEST (WatchFoldersTest, RemovedFolderIsNoLongerScanned)
{
    Fixture f;
    auto w = f.make();
    w->add (f.music);
    ASSERT_TRUE (w->waitForScans (1, kWaitMs));
    w->remove (f.music);
    writeWav (f.music.getChildFile ("c.wav"), 0.5);
    w->triggerScan();
    ASSERT_TRUE (w->waitForScans (2, kWaitMs));
    EXPECT_EQ (f.trackCount(), 0);
}

TEST (WatchFoldersTest, DeletedFileBecomesMissingOnNextPass)
{
    Fixture f;
    const auto file = writeWav (f.music.getChildFile ("d.wav"), 0.5);
    auto w = f.make();
    w->add (f.music);
    ASSERT_TRUE (w->waitForScans (1, kWaitMs));
    file.deleteFile();
    w->triggerScan();
    ASSERT_TRUE (w->waitForScans (2, kWaitMs));
    auto lib = Library::open (f.dbFile.getFullPathName(), Database::OpenMode::ReadOnly);
    EXPECT_EQ (lib->trackFiles().missingCount(), 1);
}

TEST (WatchFoldersTest, ScanOnStartRunsWithoutATrigger)
{
    Fixture f;
    {
        auto w = f.make();
        w->add (f.music);
        w->waitForScans (1, kWaitMs);
    }
    writeWav (f.music.getChildFile ("e.wav"), 0.5);
    auto w = f.make (1'000'000, true);
    ASSERT_TRUE (w->waitForScans (1, kWaitMs));
    EXPECT_EQ (f.trackCount(), 1);
}

TEST (WatchFoldersTest, DestructorStopsPromptlyWithPendingWork)
{
    Fixture f;
    for (int i = 0; i < 20; ++i)
        writeWav (f.music.getChildFile ("t" + juce::String (i) + ".wav"), 0.2, {}, 200.0f + static_cast<float> (i));
    const auto start = juce::Time::getMillisecondCounter();
    {
        auto w = f.make();
        w->add (f.music);
    }
    EXPECT_LT (juce::Time::getMillisecondCounter() - start, 5000u);
}

TEST (WatchFoldersTest, ScanThreadUsesTheSuppliedMigrationList)
{
    Fixture f;
    auto migrations = coreMigrations();
    migrations.push_back ({ 3, "extra", "CREATE TABLE extra_table (id INTEGER PRIMARY KEY);" });   // a later subsystem's table
    writeWav (f.music.getChildFile ("a.wav"), 0.5);

    WatchFolders::Options o;
    o.intervalMs = 1'000'000;
    o.pollMs = 20;
    o.scanOnStart = false;
    o.clock = [&f] { return f.fakeNow.load(); };
    o.migrations = migrations;
    std::atomic<int> failed { 0 };
    o.onScanDone = [&failed] (const IngestSummary& s) { failed += s.failed; };
    auto w = WatchFolders::create (f.dbFile.getFullPathName(), MetadataReaderRegistry::withDefaults(), o);
    ASSERT_NE (w, nullptr);
    w->add (f.music);
    ASSERT_TRUE (w->waitForScans (1, kWaitMs));
    EXPECT_EQ (failed.load(), 0);
    EXPECT_EQ (f.trackCount(), 1);
}

TEST (WatchFoldersTest, ScanThreadOpenFailureIsReported)
{
    Fixture f;
    juce::String reported;
    std::mutex m;
    std::atomic<int> failed { 0 };
    WatchFolders::Options o;
    o.intervalMs = 1'000'000;
    o.pollMs = 20;
    o.scanOnStart = false;
    o.clock = [&f] { return f.fakeNow.load(); };
    o.onError = [&] (const juce::String& e) { std::lock_guard<std::mutex> l (m); reported = e; };
    o.onScanDone = [&failed] (const IngestSummary& s) { failed += s.failed; };
    auto w = WatchFolders::create (f.dbFile.getFullPathName(), MetadataReaderRegistry::withDefaults(), o);
    ASSERT_NE (w, nullptr);

    {   // a newer app migrated the file: the scan thread's open is now refused
        auto db = Database::open (f.dbFile.getFullPathName());
        ASSERT_TRUE (db->exec ("INSERT INTO schema_version (version, name, applied_at) VALUES (99, 'future', 0);").wasOk());
    }
    w->triggerScan();
    ASSERT_TRUE (w->waitForScans (1, kWaitMs));
    EXPECT_EQ (failed.load(), 1);
    std::lock_guard<std::mutex> l (m);
    EXPECT_TRUE (reported.contains ("newer"));
}
