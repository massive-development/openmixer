// OpenMixer LibraryDatabaseTest - Database wrapper, transactions and schema migrations
#include <JuceHeader.h>
#include "library/Library.h"

#include <gtest/gtest.h>

using namespace openmixer::library;

namespace
{
std::unique_ptr<Database> memoryDb()
{
    juce::String error;
    auto db = Database::open (":memory:", Database::OpenMode::ReadWrite, &error);
    EXPECT_NE (db, nullptr) << error;
    return db;
}

int rowCount (Database& db, const char* sql)
{
    auto s = db.prepare (sql);
    return s.nextRow() ? s.integer (0) : -1;
}
} // namespace

TEST (LibraryMigrations, EmptyDatabaseMigratesToLatest)
{
    auto db = memoryDb();
    ASSERT_EQ (currentSchemaVersion (*db), 0);
    ASSERT_TRUE (migrate (*db, coreMigrations()).wasOk());

    EXPECT_EQ (currentSchemaVersion (*db), coreMigrations().back().version);
    EXPECT_EQ (rowCount (*db, "SELECT COUNT(*) FROM sqlite_master WHERE name IN"
                              " ('tracks','cues','loops','playlists','playlist_entries');"), 5);
}

TEST (LibraryMigrations, RerunningIsANoOp)
{
    auto db = memoryDb();
    ASSERT_TRUE (migrate (*db, coreMigrations()).wasOk());
    ASSERT_TRUE (migrate (*db, coreMigrations()).wasOk());
    EXPECT_EQ (rowCount (*db, "SELECT COUNT(*) FROM schema_version;"), static_cast<int> (coreMigrations().size()));
}

TEST (LibraryMigrations, AppliesOnlyNewerMigrationsInOrder)
{
    auto db = memoryDb();
    std::vector<Migration> v1 { { 1, "a", "CREATE TABLE a (x INTEGER);" } };
    ASSERT_TRUE (migrate (*db, v1).wasOk());

    std::vector<Migration> v2 = v1;
    v2.push_back ({ 2, "b", "CREATE TABLE b (x INTEGER);" });
    ASSERT_TRUE (migrate (*db, v2).wasOk());
    EXPECT_EQ (currentSchemaVersion (*db), 2);
}

TEST (LibraryMigrations, FailedMigrationRollsBackAndKeepsVersion)
{
    auto db = memoryDb();
    std::vector<Migration> list { { 1, "ok", "CREATE TABLE a (x INTEGER);" },
                                  { 2, "bad", "CREATE TABLE b (x INTEGER); CREATE TABLE a (y INTEGER);" } };
    EXPECT_TRUE (migrate (*db, list).failed());
    EXPECT_EQ (currentSchemaVersion (*db), 1);
    EXPECT_EQ (rowCount (*db, "SELECT COUNT(*) FROM sqlite_master WHERE name='b';"), 0);
}

TEST (LibraryMigrations, RejectsDowngradeAndUnorderedLists)
{
    auto db = memoryDb();
    std::vector<Migration> v2 { { 1, "a", "CREATE TABLE a (x INTEGER);" }, { 2, "b", "CREATE TABLE b (x INTEGER);" } };
    ASSERT_TRUE (migrate (*db, v2).wasOk());

    EXPECT_TRUE (migrate (*db, { v2.front() }).failed());
    EXPECT_TRUE (migrate (*db, { v2.back(), v2.front() }).failed());
}

TEST (LibraryDatabase, TransactionRollsBackUnlessCommitted)
{
    auto db = memoryDb();
    ASSERT_TRUE (db->exec ("CREATE TABLE t (x INTEGER);").wasOk());
    {
        Transaction tx (*db);
        ASSERT_TRUE (db->exec ("INSERT INTO t VALUES (1);").wasOk());
    }
    EXPECT_EQ (rowCount (*db, "SELECT COUNT(*) FROM t;"), 0);
    {
        Transaction tx (*db);
        ASSERT_TRUE (db->exec ("INSERT INTO t VALUES (1);").wasOk());
        ASSERT_TRUE (tx.commit().wasOk());
    }
    EXPECT_EQ (rowCount (*db, "SELECT COUNT(*) FROM t;"), 1);
}

TEST (LibraryDatabase, NestedTransactionRollsBackOnlyInnerScope)
{
    auto db = memoryDb();
    ASSERT_TRUE (db->exec ("CREATE TABLE t (x INTEGER);").wasOk());
    Transaction outer (*db);
    db->exec ("INSERT INTO t VALUES (1);");
    {
        Transaction inner (*db);
        db->exec ("INSERT INTO t VALUES (2);");
    }
    ASSERT_TRUE (outer.commit().wasOk());
    EXPECT_EQ (rowCount (*db, "SELECT COUNT(*) FROM t;"), 1);
}

TEST (LibraryDatabase, BadSqlYieldsInvalidStatementAndError)
{
    auto db = memoryDb();
    EXPECT_FALSE (db->prepare ("SELEKT 1").valid());
    EXPECT_TRUE (db->lastError().isNotEmpty());
    EXPECT_TRUE (db->exec ("SELEKT 1").failed());
}

TEST (LibraryDatabase, FileDatabaseUsesWalAndAllowsConcurrentReader)
{
    juce::TemporaryFile file (".db");
    auto writer = Library::open (file.getFile().getFullPathName());
    ASSERT_NE (writer, nullptr);
    TrackRecord t;
    t.path = "/music/a.wav";
    ASSERT_TRUE (writer->tracks().insert (t).has_value());

    auto s = writer->database().prepare ("PRAGMA journal_mode;");
    ASSERT_TRUE (s.nextRow());
    EXPECT_EQ (s.text (0), "wal");

    auto reader = Library::open (file.getFile().getFullPathName(), Database::OpenMode::ReadOnly);
    ASSERT_NE (reader, nullptr);
    EXPECT_EQ (reader->tracks().count(), 1);
    EXPECT_TRUE (reader->tracks().insert ({}).has_value() == false);   // read-only rejects writes
}

TEST (LibraryDatabase, OpeningNonexistentReadOnlyFileFails)
{
    juce::String error;
    EXPECT_EQ (Library::open ("/nonexistent-dir/none.db", Database::OpenMode::ReadOnly, coreMigrations(), &error), nullptr);
    EXPECT_TRUE (error.isNotEmpty());
}
