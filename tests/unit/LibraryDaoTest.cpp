// OpenMixer LibraryDaoTest - track, cue/loop and playlist DAOs
#include <JuceHeader.h>
#include "library/Library.h"

#include <gtest/gtest.h>

using namespace openmixer::library;

namespace
{
std::unique_ptr<Library> memoryLibrary()
{
    juce::String error;
    auto lib = Library::open (":memory:", Database::OpenMode::ReadWrite, coreMigrations(), &error);
    EXPECT_NE (lib, nullptr) << error;
    return lib;
}

TrackRecord sampleTrack (const juce::String& path)
{
    TrackRecord t;
    t.path = path;
    t.contentHash = "hash-" + path;
    t.title = "Title";
    t.artist = "Artist";
    t.album = "Album";
    t.genre = "House";
    t.year = 2020;
    t.durationSec = 215.5;
    t.bpm = 124.0;
    t.key = "Am";
    t.camelot = "8A";
    t.rating = 4;
    t.playCount = 3;
    t.dateAddedMs = 1000;
    t.lastPlayedMs = 2000;
    t.comment = "note";
    return t;
}

std::int64_t addTrack (Library& lib, const juce::String& path)
{
    auto id = lib.tracks().insert (sampleTrack (path));
    EXPECT_TRUE (id.has_value());
    return id.value_or (0);
}

std::vector<std::int64_t> trackOrder (Library& lib, std::int64_t playlistId)
{
    std::vector<std::int64_t> ids;
    for (const auto& e : lib.playlists().entries (playlistId))
        ids.push_back (e.trackId);
    return ids;
}

std::vector<juce::String> names (const std::vector<Playlist>& list)
{
    std::vector<juce::String> out;
    for (const auto& p : list)
        out.push_back (p.name);
    return out;
}
} // namespace

TEST (LibraryTracks, InsertAndFindRoundTripsEveryField)
{
    auto lib = memoryLibrary();
    auto id = addTrack (*lib, "/m/a.wav");

    auto got = lib->tracks().findById (id);
    ASSERT_TRUE (got.has_value());
    const auto want = sampleTrack ("/m/a.wav");
    EXPECT_EQ (got->path, want.path);
    EXPECT_EQ (got->contentHash, want.contentHash);
    EXPECT_EQ (got->title, want.title);
    EXPECT_EQ (got->artist, want.artist);
    EXPECT_EQ (got->album, want.album);
    EXPECT_EQ (got->genre, want.genre);
    EXPECT_EQ (got->year, 2020);
    EXPECT_DOUBLE_EQ (got->durationSec, 215.5);
    EXPECT_DOUBLE_EQ (got->bpm.value_or (0), 124.0);
    EXPECT_EQ (got->key, "Am");
    EXPECT_EQ (got->camelot, "8A");
    EXPECT_EQ (got->rating, 4);
    EXPECT_EQ (got->playCount, 3);
    EXPECT_EQ (got->dateAddedMs, 1000);
    EXPECT_EQ (got->lastPlayedMs.value_or (0), 2000);
    EXPECT_EQ (got->comment, "note");
}

TEST (LibraryTracks, UnicodeAndNullableFieldsSurvive)
{
    auto lib = memoryLibrary();
    TrackRecord t;
    t.path = "/m/\xE2\x99\xAB.wav";
    t.title = juce::String::fromUTF8 ("Caf\xC3\xA9 \xE2\x99\xAB");
    auto id = lib->tracks().insert (t);
    ASSERT_TRUE (id.has_value());

    auto got = lib->tracks().findByPath (t.path);
    ASSERT_TRUE (got.has_value());
    EXPECT_EQ (got->title, t.title);
    EXPECT_FALSE (got->bpm.has_value());
    EXPECT_FALSE (got->lastPlayedMs.has_value());
    EXPECT_GT (got->dateAddedMs, 0);   // defaulted to now
}

TEST (LibraryTracks, DuplicatePathIsRejected)
{
    auto lib = memoryLibrary();
    addTrack (*lib, "/m/a.wav");
    EXPECT_FALSE (lib->tracks().insert (sampleTrack ("/m/a.wav")).has_value());
    EXPECT_EQ (lib->tracks().count(), 1);
}

TEST (LibraryTracks, UpdateAndNarrowWriters)
{
    auto lib = memoryLibrary();
    auto id = addTrack (*lib, "/m/a.wav");
    auto t = *lib->tracks().findById (id);
    t.title = "New";
    t.bpm.reset();
    ASSERT_TRUE (lib->tracks().update (t).wasOk());
    EXPECT_EQ (lib->tracks().findById (id)->title, "New");
    EXPECT_FALSE (lib->tracks().findById (id)->bpm.has_value());

    ASSERT_TRUE (lib->tracks().setAnalysis (id, 128.0, "C", "8B").wasOk());
    ASSERT_TRUE (lib->tracks().setRating (id, 5).wasOk());
    ASSERT_TRUE (lib->tracks().recordPlay (id, 9999).wasOk());
    auto got = *lib->tracks().findById (id);
    EXPECT_DOUBLE_EQ (*got.bpm, 128.0);
    EXPECT_EQ (got.camelot, "8B");
    EXPECT_EQ (got.rating, 5);
    EXPECT_EQ (got.playCount, 4);
    EXPECT_EQ (*got.lastPlayedMs, 9999);
}

TEST (LibraryTracks, RatingOutOfRangeAndUnknownIdFail)
{
    auto lib = memoryLibrary();
    auto id = addTrack (*lib, "/m/a.wav");
    EXPECT_TRUE (lib->tracks().setRating (id, 6).failed());
    EXPECT_TRUE (lib->tracks().setRating (id + 100, 3).failed());
    EXPECT_TRUE (lib->tracks().remove (id + 100).failed());
}

TEST (LibraryTracks, UpsertKeepsUserFieldsAndId)
{
    auto lib = memoryLibrary();
    auto id = addTrack (*lib, "/m/a.wav");

    auto refreshed = sampleTrack ("/m/a.wav");
    refreshed.title = "Retagged";
    refreshed.rating = 0;
    refreshed.playCount = 0;
    auto again = lib->tracks().upsertByPath (refreshed);
    ASSERT_TRUE (again.has_value());
    EXPECT_EQ (*again, id);

    auto got = *lib->tracks().findById (id);
    EXPECT_EQ (got.title, "Retagged");
    EXPECT_EQ (got.rating, 4);
    EXPECT_EQ (got.playCount, 3);
    EXPECT_EQ (lib->tracks().count(), 1);

    EXPECT_TRUE (lib->tracks().upsertByPath (sampleTrack ("/m/b.wav")).has_value());
    EXPECT_EQ (lib->tracks().count(), 2);
}

TEST (LibraryTracks, FindByContentHashReturnsAllCopies)
{
    auto lib = memoryLibrary();
    auto a = sampleTrack ("/m/a.wav");
    auto b = sampleTrack ("/m/b.wav");
    a.contentHash = b.contentHash = "same";
    lib->tracks().insert (a);
    lib->tracks().insert (b);
    EXPECT_EQ (lib->tracks().findByContentHash ("same").size(), 2u);
    EXPECT_TRUE (lib->tracks().findByContentHash ("other").empty());
    EXPECT_EQ (lib->tracks().all().size(), 2u);
}

TEST (LibraryCues, SetReplaceListAndClear)
{
    auto lib = memoryLibrary();
    auto id = addTrack (*lib, "/m/a.wav");
    auto& cues = lib->cues();
    ASSERT_TRUE (cues.setCue ({ id, 2, 44100, 0xFFFF0000u, "drop" }).wasOk());
    ASSERT_TRUE (cues.setCue ({ id, 0, 100, 0xFF00FF00u, "intro" }).wasOk());
    ASSERT_TRUE (cues.setCue ({ id, 2, 88200, 0xFF0000FFu, "drop2" }).wasOk());

    auto list = cues.cues (id);
    ASSERT_EQ (list.size(), 2u);
    EXPECT_EQ (list[0].slot, 0);
    EXPECT_EQ (list[1].positionFrames, 88200);
    EXPECT_EQ (list[1].colourArgb, 0xFF0000FFu);
    EXPECT_EQ (list[1].label, "drop2");

    ASSERT_TRUE (cues.clearCue (id, 0).wasOk());
    EXPECT_EQ (cues.cues (id).size(), 1u);
}

TEST (LibraryCues, LoopsRoundTripAndRejectInvertedRange)
{
    auto lib = memoryLibrary();
    auto id = addTrack (*lib, "/m/a.wav");
    auto& cues = lib->cues();
    ASSERT_TRUE (cues.setLoop ({ id, 1, 1000, 5000, 0xFFAABBCCu, "loop" }).wasOk());
    EXPECT_TRUE (cues.setLoop ({ id, 2, 5000, 1000, 0, "" }).failed());

    auto list = cues.loops (id);
    ASSERT_EQ (list.size(), 1u);
    EXPECT_EQ (list[0].startFrames, 1000);
    EXPECT_EQ (list[0].endFrames, 5000);
    ASSERT_TRUE (cues.clearLoop (id, 1).wasOk());
    EXPECT_TRUE (cues.loops (id).empty());
}

TEST (LibraryCues, UnknownTrackFailsAndDeleteCascades)
{
    auto lib = memoryLibrary();
    EXPECT_TRUE (lib->cues().setCue ({ 999, 0, 1, 0, "" }).failed());

    auto id = addTrack (*lib, "/m/a.wav");
    lib->cues().setCue ({ id, 0, 1, 0, "" });
    lib->cues().setLoop ({ id, 0, 1, 2, 0, "" });
    ASSERT_TRUE (lib->tracks().remove (id).wasOk());
    EXPECT_TRUE (lib->cues().cues (id).empty());
    EXPECT_TRUE (lib->cues().loops (id).empty());
}

TEST (LibraryPlaylists, EntriesAppendInsertMoveRemoveKeepDensePositions)
{
    auto lib = memoryLibrary();
    auto pl = lib->playlists().create ("Set");
    ASSERT_TRUE (pl.has_value());
    const auto a = addTrack (*lib, "/a"), b = addTrack (*lib, "/b"), c = addTrack (*lib, "/c"), d = addTrack (*lib, "/d");

    auto& p = lib->playlists();
    auto ea = p.addTrack (*pl, a);
    p.addTrack (*pl, b);
    p.addTrack (*pl, c);
    EXPECT_EQ (trackOrder (*lib, *pl), (std::vector<std::int64_t> { a, b, c }));

    p.addTrack (*pl, d, 1);
    EXPECT_EQ (trackOrder (*lib, *pl), (std::vector<std::int64_t> { a, d, b, c }));

    ASSERT_TRUE (p.moveEntry (*ea, 3).wasOk());
    EXPECT_EQ (trackOrder (*lib, *pl), (std::vector<std::int64_t> { d, b, c, a }));
    ASSERT_TRUE (p.moveEntry (*ea, 0).wasOk());
    EXPECT_EQ (trackOrder (*lib, *pl), (std::vector<std::int64_t> { a, d, b, c }));
    ASSERT_TRUE (p.moveEntry (*ea, 99).wasOk());   // clamps to the end
    EXPECT_EQ (trackOrder (*lib, *pl), (std::vector<std::int64_t> { d, b, c, a }));

    ASSERT_TRUE (p.removeEntry (p.entries (*pl)[1].id).wasOk());
    EXPECT_EQ (trackOrder (*lib, *pl), (std::vector<std::int64_t> { d, c, a }));
    const auto remaining = p.entries (*pl);
    for (size_t i = 0; i < remaining.size(); ++i)
        EXPECT_EQ (remaining[i].position, static_cast<int> (i));
}

TEST (LibraryPlaylists, SameTrackMayAppearTwiceAndDeletingTrackRemovesEntries)
{
    auto lib = memoryLibrary();
    auto pl = *lib->playlists().create ("Set");
    auto a = addTrack (*lib, "/a");
    lib->playlists().addTrack (pl, a);
    lib->playlists().addTrack (pl, a);
    EXPECT_EQ (lib->playlists().entries (pl).size(), 2u);

    lib->tracks().remove (a);
    EXPECT_TRUE (lib->playlists().entries (pl).empty());
}

TEST (LibraryPlaylists, AddTrackRejectsFolderAndUnknownTrackWithoutSideEffects)
{
    auto lib = memoryLibrary();
    auto folder = *lib->playlists().create ("F", std::nullopt, true);
    auto pl = *lib->playlists().create ("P");
    auto a = addTrack (*lib, "/a");
    lib->playlists().addTrack (pl, a);

    EXPECT_FALSE (lib->playlists().addTrack (folder, a).has_value());
    EXPECT_FALSE (lib->playlists().addTrack (pl, 9999, 0).has_value());
    ASSERT_EQ (lib->playlists().entries (pl).size(), 1u);
    EXPECT_EQ (lib->playlists().entries (pl)[0].position, 0);   // failed insert at 0 did not shift
}

TEST (LibraryPlaylists, NestingOrderingAndCascadeDelete)
{
    auto lib = memoryLibrary();
    auto& p = lib->playlists();
    auto folder = *p.create ("Folder", std::nullopt, true);
    auto x = *p.create ("X", folder);
    auto y = *p.create ("Y", folder);
    auto z = *p.create ("Z", folder);
    EXPECT_EQ (names (p.children (folder)), (std::vector<juce::String> { "X", "Y", "Z" }));

    ASSERT_TRUE (p.move (z, folder, 0).wasOk());
    EXPECT_EQ (names (p.children (folder)), (std::vector<juce::String> { "Z", "X", "Y" }));

    ASSERT_TRUE (p.move (x, std::nullopt, 0).wasOk());   // out of the folder, to the top
    EXPECT_EQ (names (p.children (std::nullopt)), (std::vector<juce::String> { "X", "Folder" }));
    EXPECT_EQ (names (p.children (folder)), (std::vector<juce::String> { "Z", "Y" }));
    EXPECT_EQ (p.children (folder)[1].position, 1);

    auto t = addTrack (*lib, "/a");
    p.addTrack (y, t);
    ASSERT_TRUE (p.remove (folder).wasOk());
    EXPECT_FALSE (p.find (y).has_value());
    EXPECT_FALSE (p.findEntry (1).has_value());
    EXPECT_EQ (names (p.children (std::nullopt)), (std::vector<juce::String> { "X" }));
    EXPECT_EQ (p.children (std::nullopt)[0].position, 0);
}

TEST (LibraryPlaylists, RejectsCyclesAndNonFolderParents)
{
    auto lib = memoryLibrary();
    auto& p = lib->playlists();
    auto outer = *p.create ("Outer", std::nullopt, true);
    auto inner = *p.create ("Inner", outer, true);
    auto leaf = *p.create ("Leaf", inner);

    EXPECT_TRUE (p.move (outer, inner, 0).failed());
    EXPECT_TRUE (p.move (outer, outer, 0).failed());
    EXPECT_TRUE (p.move (inner, leaf, 0).failed());
    EXPECT_FALSE (p.create ("Bad", leaf).has_value());
    EXPECT_EQ (p.find (inner)->parentId.value_or (0), outer);
}

TEST (LibraryPlaylists, RenameAndFailedMoveRollsBack)
{
    auto lib = memoryLibrary();
    auto& p = lib->playlists();
    auto a = *p.create ("A");
    auto b = *p.create ("B");
    ASSERT_TRUE (p.rename (a, "Renamed").wasOk());
    EXPECT_TRUE (p.rename (999, "x").failed());
    EXPECT_TRUE (p.move (a, 999, 0).failed());
    EXPECT_EQ (names (p.children (std::nullopt)), (std::vector<juce::String> { "Renamed", "B" }));
    EXPECT_EQ (p.find (b)->position, 1);
}

TEST (LibraryTracks, EmptyStringHashLookupMatchesDefaultColumn)
{
    auto lib = memoryLibrary();
    TrackRecord t;
    t.path = "/m/nohash.wav";   // content_hash defaults to ''
    ASSERT_TRUE (lib->tracks().insert (t).has_value());

    juce::Result status = juce::Result::ok();
    EXPECT_EQ (lib->tracks().findByContentHash ("", &status).size(), 1u);
    EXPECT_TRUE (status.wasOk());
    EXPECT_TRUE (lib->tracks().findByPath ("", &status) == std::nullopt);
}

TEST (LibraryTracks, FindByIdUsesBoundParameter)
{
    auto lib = memoryLibrary();
    auto id = addTrack (*lib, "/m/a.wav");
    EXPECT_TRUE (lib->tracks().findById (id).has_value());
    EXPECT_FALSE (lib->tracks().findById (id + 1).has_value());
}

TEST (LibraryReads, FailedReadsAreDistinguishableFromEmpty)
{
    auto lib = memoryLibrary();
    juce::Result status = juce::Result::ok();
    EXPECT_TRUE (lib->tracks().all (&status).empty());
    EXPECT_TRUE (status.wasOk());   // genuinely empty

    ASSERT_TRUE (lib->database().exec ("DROP TABLE playlist_entries; DROP TABLE cues; DROP TABLE loops;"
                                       " DROP TABLE tracks;").wasOk());
    EXPECT_TRUE (lib->tracks().all (&status).empty());
    EXPECT_TRUE (status.failed());

    status = juce::Result::ok();
    lib->tracks().count (&status);
    EXPECT_TRUE (status.failed());
    status = juce::Result::ok();
    lib->tracks().findById (1, &status);
    EXPECT_TRUE (status.failed());
    status = juce::Result::ok();
    lib->cues().cues (1, &status);
    EXPECT_TRUE (status.failed());
    status = juce::Result::ok();
    lib->playlists().entries (1, &status);
    EXPECT_TRUE (status.failed());
    EXPECT_FALSE (lib->tracks().upsertByPath (sampleTrack ("/x")).has_value());
}
