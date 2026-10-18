// OpenMixer LibrarySearchTest - query parser, SQL compilation and results over a seeded library
#include <JuceHeader.h>
#include "library/Library.h"
#include "library/SearchEngine.h"

#include <gtest/gtest.h>

using namespace openmixer::library;

namespace
{
std::int64_t utcMs (int y, int m, int d) { return juce::Time (y, m - 1, d, 0, 0, 0, 0, false).toMilliseconds(); }

struct Seed
{
    const char* title; const char* artist; const char* genre; std::optional<double> bpm; const char* camelot;
    int rating; std::int64_t added; const char* comment;
};

struct Fixture
{
    Fixture()
    {
        juce::String error;
        lib = Library::open (":memory:", Database::OpenMode::ReadWrite, coreMigrations(), &error);
        EXPECT_NE (lib, nullptr) << error;
        seed();
        engine = std::make_unique<SearchEngine> (lib->database());
        engine->clock = [] { return utcMs (2024, 3, 21); };
    }

    void seed()
    {
        const Seed rows[] = {
            { "Alpha", "Daft", "House", 124.0, "8A", 5, utcMs (2024, 1, 10), "" },
            { "Bravo", "Moby", "Techno", 128.0, "9A", 3, utcMs (2024, 2, 1), "" },
            { "Charlie", "Moby", "House", 132.0, "8B", 4, utcMs (2024, 3, 1), "" },
            { "Delta", "100%_Pure", "Ambient", std::nullopt, "", 0, utcMs (2024, 3, 15), "late night" },
            { "Echo", "Daft", "Techno", 140.0, "3A", 1, utcMs (2024, 3, 20), "" },
        };
        for (const auto& r : rows)
        {
            TrackRecord t;
            t.path = juce::String ("/m/") + r.title + ".wav";
            t.title = r.title; t.artist = r.artist; t.genre = r.genre; t.bpm = r.bpm;
            t.camelot = r.camelot; t.rating = r.rating; t.dateAddedMs = r.added; t.comment = r.comment;
            ids.push_back (*lib->tracks().insert (t));
        }
        const auto warm = *lib->playlists().create ("Warmup", std::nullopt, false);
        const auto peak = *lib->playlists().create ("Peak", std::nullopt, false);
        lib->playlists().addTrack (warm, ids[0]);
        lib->playlists().addTrack (warm, ids[1]);
        lib->playlists().addTrack (peak, ids[2]);
        peakId = peak;
    }

    juce::StringArray titles (const juce::String& q, juce::Result* status = nullptr)
    {
        juce::Result local = juce::Result::ok();
        auto rows = engine->search (q, status != nullptr ? status : &local);
        EXPECT_TRUE ((status != nullptr ? *status : local).wasOk()) << q;
        juce::StringArray out;
        for (const auto& r : rows)
            out.add (r.title);
        return out;
    }

    std::string joined (const juce::String& q) { return titles (q).joinIntoString (",").toStdString(); }

    std::unique_ptr<Library> lib;
    std::unique_ptr<SearchEngine> engine;
    std::vector<std::int64_t> ids;
    std::int64_t peakId = 0;
};
} // namespace

// ---- parser ----

TEST (LibrarySearchParser, SplitsFieldsFreeTextAndQuotes)
{
    const auto fields = SearchFieldRegistry::withDefaults();
    const auto p = parseQuery ("artist:foo \"deep house\" bpm:120-128 12:30", fields);
    ASSERT_EQ (p.query.terms.size(), 4u);
    EXPECT_EQ (p.query.terms[0].field, "artist");
    EXPECT_EQ (p.query.terms[0].value, "foo");
    EXPECT_TRUE (p.query.terms[1].field.isEmpty());
    EXPECT_EQ (p.query.terms[1].value, "deep house");
    EXPECT_EQ (p.query.terms[2].value, "120-128");
    EXPECT_TRUE (p.query.terms[3].field.isEmpty());   // unknown prefix stays free text
    EXPECT_EQ (p.query.terms[3].value, "12:30");
    EXPECT_TRUE (p.errors.isEmpty());
}

TEST (LibrarySearchParser, QuotedFieldValueAndNegation)
{
    const auto p = parseQuery ("-genre:techno title:\"a b\" -\"x y\"", SearchFieldRegistry::withDefaults());
    ASSERT_EQ (p.query.terms.size(), 3u);
    EXPECT_TRUE (p.query.terms[0].negated);
    EXPECT_EQ (p.query.terms[0].field, "genre");
    EXPECT_FALSE (p.query.terms[1].negated);
    EXPECT_EQ (p.query.terms[1].value, "a b");
    EXPECT_TRUE (p.query.terms[2].negated);
    EXPECT_EQ (p.query.terms[2].value, "x y");
}

TEST (LibrarySearchParser, SortLimitOffsetAreReserved)
{
    const auto p = parseQuery ("sort:-BPM limit:10 offset:20 house", SearchFieldRegistry::withDefaults());
    EXPECT_EQ (p.query.sortField, "bpm");
    EXPECT_TRUE (p.query.descending);
    EXPECT_EQ (p.query.limit, 10);
    EXPECT_EQ (p.query.offset, 20);
    EXPECT_EQ (p.query.terms.size(), 1u);
}

TEST (LibrarySearchParser, BadLimitIsReportedNotIgnored)
{
    const auto p = parseQuery ("limit:abc offset:-3 house", SearchFieldRegistry::withDefaults());
    EXPECT_EQ (p.errors.size(), 2);
    EXPECT_EQ (p.query.terms.size(), 1u);
}

TEST (LibrarySearchParser, EmptyAndWhitespaceQueriesHaveNoTerms)
{
    EXPECT_TRUE (parseQuery ("", SearchFieldRegistry::withDefaults()).query.terms.empty());
    EXPECT_TRUE (parseQuery ("   \t ", SearchFieldRegistry::withDefaults()).query.terms.empty());
}

// ---- results ----

TEST (LibrarySearch, EmptyQueryReturnsEverythingById)
{
    Fixture f;
    EXPECT_EQ (f.joined (""), "Alpha,Bravo,Charlie,Delta,Echo");
}

TEST (LibrarySearch, FreeTextSearchesAllTextFieldsCaseInsensitively)
{
    Fixture f;
    EXPECT_EQ (f.joined ("MOBY"), "Bravo,Charlie");
    EXPECT_EQ (f.joined ("late"), "Delta");           // comment
    EXPECT_EQ (f.joined ("ambient"), "Delta");        // genre
    EXPECT_EQ (f.joined ("moby house"), "Charlie");   // terms AND together
}

TEST (LibrarySearch, FieldFilters)
{
    Fixture f;
    EXPECT_EQ (f.joined ("artist:daft"), "Alpha,Echo");
    EXPECT_EQ (f.joined ("genre:=house"), "Alpha,Charlie");
    EXPECT_EQ (f.joined ("-genre:techno artist:moby"), "Charlie");
    EXPECT_EQ (f.joined ("title:\"lph\""), "Alpha");
}

TEST (LibrarySearch, LikeWildcardsInUserTextAreLiteral)
{
    Fixture f;
    EXPECT_EQ (f.joined ("artist:%"), "Delta");
    EXPECT_EQ (f.joined ("artist:_"), "Delta");
    EXPECT_EQ (f.joined ("artist:0%_P"), "Delta");
}

TEST (LibrarySearch, BpmRangesAndComparisons)
{
    Fixture f;
    EXPECT_EQ (f.joined ("bpm:124-128"), "Alpha,Bravo");
    EXPECT_EQ (f.joined ("bpm:128..140"), "Bravo,Charlie,Echo");
    EXPECT_EQ (f.joined ("bpm:>128"), "Charlie,Echo");
    EXPECT_EQ (f.joined ("bpm:<=128"), "Alpha,Bravo");
    EXPECT_EQ (f.joined ("bpm:132"), "Charlie");
    EXPECT_EQ (f.joined ("-bpm:124-132"), "Delta,Echo");   // unanalysed tracks are outside any range
}

TEST (LibrarySearch, RatingFilter)
{
    Fixture f;
    EXPECT_EQ (f.joined ("rating:>=4"), "Alpha,Charlie");
    EXPECT_EQ (f.joined ("rating:0"), "Delta");
}

TEST (LibrarySearch, KeyExactAndCompatible)
{
    Fixture f;
    EXPECT_EQ (f.joined ("key:8A"), "Alpha");
    EXPECT_EQ (f.joined ("key:8a"), "Alpha");
    EXPECT_EQ (f.joined ("key:Am"), "Alpha");          // musical name resolves to 8A
    EXPECT_EQ (f.joined ("key:~8A"), "Alpha,Bravo,Charlie");   // 8A, 9A (+1), 8B (relative)
    EXPECT_EQ (f.joined ("-key:~8A"), "Delta,Echo");
}

TEST (LibrarySearch, DateAddedAbsoluteAndRelative)
{
    Fixture f;
    EXPECT_EQ (f.joined ("added:2024-03-01"), "Charlie");
    EXPECT_EQ (f.joined ("added:>2024-03-01"), "Delta,Echo");
    EXPECT_EQ (f.joined ("added:>=2024-03-01"), "Charlie,Delta,Echo");
    EXPECT_EQ (f.joined ("added:2024-02-01..2024-03-01"), "Bravo,Charlie");
    EXPECT_EQ (f.joined ("added:7d"), "Delta,Echo");   // clock is 2024-03-21
    EXPECT_EQ (f.joined ("added:<2024-02-01"), "Alpha");
}

TEST (LibrarySearch, PlaylistMembershipByNameAndId)
{
    Fixture f;
    EXPECT_EQ (f.joined ("playlist:warmup"), "Alpha,Bravo");
    EXPECT_EQ (f.joined ("playlist:=Peak"), "Charlie");
    EXPECT_EQ (f.joined ("playlist:#" + juce::String (f.peakId)), "Charlie");
    EXPECT_EQ (f.joined ("-playlist:warmup"), "Charlie,Delta,Echo");
}

TEST (LibrarySearch, SortsByAnyRegisteredColumn)
{
    Fixture f;
    EXPECT_EQ (f.joined ("sort:-bpm"), "Echo,Charlie,Bravo,Alpha,Delta");
    EXPECT_EQ (f.joined ("sort:rating"), "Delta,Echo,Bravo,Charlie,Alpha");
    EXPECT_EQ (f.joined ("sort:-title"), "Echo,Delta,Charlie,Bravo,Alpha");
    EXPECT_EQ (f.joined ("sort:key bpm:>0"), "Echo,Alpha,Charlie,Bravo");   // 3A 8A 8B 9A: wheel number, then letter
    EXPECT_EQ (f.joined ("sort:artist"), "Delta,Alpha,Echo,Bravo,Charlie");
}

TEST (LibrarySearch, LimitOffsetPageAndCountIgnoresPaging)
{
    Fixture f;
    EXPECT_EQ (f.joined ("limit:2"), "Alpha,Bravo");
    EXPECT_EQ (f.joined ("limit:2 offset:2"), "Charlie,Delta");
    EXPECT_EQ (f.joined ("offset:4"), "Echo");
    EXPECT_EQ (f.joined ("offset:99"), "");

    auto q = f.engine->parse ("genre:house limit:1").query;
    juce::Result r = juce::Result::ok();
    EXPECT_EQ (f.engine->count (q, &r), 2);
    EXPECT_TRUE (r.wasOk());
}

// ---- failure reporting ----

TEST (LibrarySearch, BadQueriesFailInsteadOfReturningNoResults)
{
    Fixture f;
    for (const char* q : { "sort:nonsense", "bpm:fast", "key:H#", "added:yesterday", "playlist:#x", "limit:zzz", "sort:", "sort:-" })
    {
        juce::Result r = juce::Result::ok();
        EXPECT_TRUE (f.engine->search (juce::String (q), &r).empty()) << q;
        EXPECT_TRUE (r.failed()) << q;
    }
}

TEST (LibrarySearch, UnknownFieldInHandBuiltQueryFails)
{
    Fixture f;
    SearchQuery q;
    q.terms.push_back ({ "nope", "x", false });
    juce::Result r = juce::Result::ok();
    EXPECT_TRUE (f.engine->search (q, &r).empty());
    EXPECT_TRUE (r.failed());
    EXPECT_EQ (f.engine->count (q, &r), -1);
}

TEST (LibrarySearch, DatabaseFailureIsSurfacedNotEmpty)
{
    Fixture f;
    ASSERT_TRUE (f.lib->database().exec ("ALTER TABLE tracks RENAME TO tracks_gone;").wasOk());
    juce::Result r = juce::Result::ok();
    EXPECT_TRUE (f.engine->search ("alpha", &r).empty());
    EXPECT_TRUE (r.failed());
    r = juce::Result::ok();
    EXPECT_EQ (f.engine->count ({}, &r), -1);
    EXPECT_TRUE (r.failed());
}

// ---- injection safety ----

TEST (LibrarySearch, HostileValuesAreDataNotSql)
{
    Fixture f;
    const char* hostile[] = { "'; DROP TABLE tracks; --", "\" OR 1=1 --", "x') OR ('1'='1", "Robert'); DELETE FROM tracks;--" };
    for (const char* h : hostile)
        for (const char* field : { "", "title:", "artist:", "genre:=", "playlist:", "key:", "bpm:", "added:" })
        {
            juce::Result r = juce::Result::ok();
            const auto rows = f.engine->search (juce::String (field) + "\"" + h + "\"", &r);
            EXPECT_TRUE (rows.empty()) << field << h;   // matches nothing, or fails to parse a number/key/date
        }
    EXPECT_EQ (f.lib->tracks().count(), 5);
    EXPECT_EQ (f.joined (""), "Alpha,Bravo,Charlie,Delta,Echo");
}

TEST (LibrarySearch, HostileSortFieldIsRejected)
{
    Fixture f;
    SearchQuery q;
    q.sortField = "id; DROP TABLE tracks";
    juce::Result r = juce::Result::ok();
    EXPECT_TRUE (f.engine->search (q, &r).empty());
    EXPECT_TRUE (r.failed());
    EXPECT_EQ (f.lib->tracks().count(), 5);
}

TEST (LibrarySearch, CompiledSqlNeverContainsUserText)
{
    const auto fields = SearchFieldRegistry::withDefaults();
    const auto p = parseQuery ("artist:\"zz9plural'; --\" bpm:120-128 key:~8A playlist:\"zz9z\" sort:-bpm", fields);
    CompiledSearch c;
    ASSERT_TRUE (compileSearch (p.query, fields, {}, c).wasOk());
    EXPECT_EQ (c.selectSql.find ("zz9"), std::string::npos);
    EXPECT_EQ (c.selectSql.find ("120"), std::string::npos);
    EXPECT_EQ (c.selectSql.find ("8A"), std::string::npos);
    EXPECT_EQ (std::count (c.selectSql.begin(), c.selectSql.end(), '?'), static_cast<long> (c.selectArgs.size()));
}

// ---- extension point ----

TEST (LibrarySearch, PluginCanRegisterAField)
{
    Fixture f;
    SearchField longTracks;
    longTracks.name = "has-comment";
    longTracks.kind = FieldKind::Custom;
    longTracks.compile = [] (const FilterTerm& t, SqlFragment& out)
    {
        out.sql += std::string ("tracks.comment ") + (t.value == "no" ? "= ''" : "<> ''");
        return juce::Result::ok();
    };
    f.engine->registry().add (longTracks);

    EXPECT_EQ (f.joined ("has-comment:yes"), "Delta");
    EXPECT_EQ (f.joined ("has-comment:no"), "Alpha,Bravo,Charlie,Echo");
}

TEST (LibrarySearch, MigrationAddsIndexes)
{
    Fixture f;
    auto s = f.lib->database().prepare ("SELECT COUNT(*) FROM sqlite_master WHERE name='idx_tracks_bpm';");
    ASSERT_TRUE (s.nextRow());
    EXPECT_EQ (s.integer (0), 1);
}
