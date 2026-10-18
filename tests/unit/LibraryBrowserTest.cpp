// OpenMixer LibraryBrowserTest - track table model, search box, sidebar tree, services wiring (needs GUI init from TestMain)
#include <JuceHeader.h>
#include "LibraryUiTestUtil.h"
#include "ui/DeckWidget.h"
#include "ui/GlassLookAndFeel.h"
#include "ui/LibraryBrowser.h"
#include "ui/ThemeRegistry.h"
#include "controllers/ParameterRouter.h"

#include <gtest/gtest.h>

#include <set>

using namespace openmixer;
using namespace openmixer::ui;
using namespace libui;

namespace
{
// Counts how often the table asks for rows, to prove paging.
struct CountingSource final : TrackSource
{
    explicit CountingSource (int rows, int* fetchCounter) : total (rows), fetches (fetchCounter) {}
    int count() override { return total; }
    std::vector<library::TrackRecord> fetch (int offset, int limit) override
    {
        ++*fetches;
        std::vector<library::TrackRecord> out;
        for (int i = offset; i < std::min (total, offset + limit); ++i)
        {
            library::TrackRecord t;
            t.id = i + 1;
            t.title = "T" + juce::String (i);
            out.push_back (t);
        }
        return out;
    }
    int total;
    int* fetches;
};

juce::File importFixture (const char* name)
{
    return juce::File (OPENMIXER_TEST_DATA_DIR).getChildFile ("import").getChildFile (name);
}

int distinctPixels (const juce::Image& image)
{
    std::set<juce::uint32> seen;
    for (int y = 0; y < image.getHeight(); ++y)
        for (int x = 0; x < image.getWidth(); ++x)
            seen.insert (image.getPixelAt (x, y).getARGB());
    return static_cast<int> (seen.size());
}

struct BrowserRig
{
    BrowserRig() : services (makeServices()), ids (seedFour (services->getLibrary())), browser (*services, 2)
    {
        browser.setSize (1200, 300);
        browser.refresh();
    }
    juce::String title (int row) { return browser.getModel().cellText (row, 1); }
    std::unique_ptr<LibraryServices> services;
    std::vector<std::int64_t> ids;
    LibraryBrowser browser;
};
}

TEST (LibraryBrowserTest, TableShowsSeededTracksWithFormattedCells)
{
    BrowserRig rig;
    auto& model = rig.browser.getModel();
    ASSERT_EQ (model.getNumRows(), 4);
    EXPECT_EQ (model.cellText (0, 1), "Alpha");
    EXPECT_EQ (model.cellText (0, 2), "Zed");
    EXPECT_EQ (model.cellText (0, 3), "120.0");
    EXPECT_EQ (model.cellText (0, 4), "8A");
    EXPECT_EQ (model.cellText (0, 5), "4:05");
    EXPECT_EQ (model.cellText (0, 7), "House");
    EXPECT_EQ (model.cellText (3, 3), "--");   // not analysed yet
    EXPECT_EQ (model.cellText (0, 8).length(), 10);   // yyyy-mm-dd
    EXPECT_EQ (rig.browser.getStatusLabel().getText(), "4 tracks");
}

TEST (LibraryBrowserTest, SearchBoxUsesTheQuerySyntax)
{
    BrowserRig rig;
    auto& model = rig.browser.getModel();
    rig.browser.setSearchText ("genre:house");
    EXPECT_EQ (model.getNumRows(), 2);
    rig.browser.setSearchText ("bpm:125-130");
    ASSERT_EQ (model.getNumRows(), 1);
    EXPECT_EQ (model.cellText (0, 1), "Bravo");
    rig.browser.setSearchText ("genre:house -artist:zed");
    ASSERT_EQ (model.getNumRows(), 1);
    EXPECT_EQ (model.cellText (0, 1), "Charlie");
    rig.browser.setSearchText ("key:~8A");   // 8A, 9A, 7A, 8B
    EXPECT_EQ (model.getNumRows(), 2);
    rig.browser.setSearchText ("sort:-bpm genre:house");
    EXPECT_EQ (model.cellText (0, 1), "Charlie");
    rig.browser.setSearchText ({});
    EXPECT_EQ (model.getNumRows(), 4);
}

TEST (LibraryBrowserTest, QueryProblemsShowInTheStatusLine)
{
    BrowserRig rig;
    rig.browser.setSearchText ("sort:");
    EXPECT_NE (rig.browser.getStatusLabel().getText().indexOf ("sort"), -1) << rig.browser.getStatusLabel().getText().toStdString();
    rig.browser.setSearchText ("alpha");
    EXPECT_EQ (rig.browser.getStatusLabel().getText(), "1 tracks");
}

TEST (LibraryBrowserTest, TypingInTheSearchBoxIsAppliedByTheDebounceTick)
{
    BrowserRig rig;
    rig.browser.getSearchBox().setText ("bravo", juce::dontSendNotification);
    rig.browser.getSearchBox().onTextChange();   // JUCE posts the real notification asynchronously
    EXPECT_EQ (rig.browser.getModel().getNumRows(), 4);   // not yet
    rig.browser.flushPending();
    EXPECT_EQ (rig.browser.getModel().getNumRows(), 1);
}

TEST (LibraryBrowserTest, HeaderClicksSortThroughTheSearchEngine)
{
    BrowserRig rig;
    auto& model = rig.browser.getModel();   // the table forwards header clicks here (JUCE does it asynchronously)
    model.sortOrderChanged (3, false);   // BPM, descending
    EXPECT_EQ (rig.title (0), "Bravo");
    EXPECT_EQ (rig.title (1), "Charlie");
    model.sortOrderChanged (3, true);
    EXPECT_EQ (rig.title (3), "Bravo");
    model.sortOrderChanged (2, true);    // artist ascending: Wu, Xi, Yan, Zed
    EXPECT_EQ (rig.title (0), "Delta");
    EXPECT_EQ (rig.title (3), "Alpha");
    model.sortOrderChanged (1, false);   // title descending
    EXPECT_EQ (rig.title (0), "Delta");
    model.sortOrderChanged (6, true);    // rating ascending
    EXPECT_EQ (rig.title (0), "Delta");
    EXPECT_EQ (rig.title (3), "Alpha");
    for (const auto& c : TrackTableModel::columns())   // every sortable column maps to a registered search field
        if (c.sortField != nullptr)
            EXPECT_NE (rig.services->getSearch().registry().find (c.sortField), nullptr) << c.sortField;
}

TEST (LibraryBrowserTest, RowsArePagedAndOnlyFetchedWhenShown)
{
    int fetches = 0;
    TrackTableModel model;
    model.setSource (std::make_unique<CountingSource> (1000, &fetches));
    EXPECT_EQ (model.getNumRows(), 1000);
    EXPECT_EQ (fetches, 0);   // counting does not read rows
    ASSERT_NE (model.rowAt (0), nullptr);
    ASSERT_NE (model.rowAt (99), nullptr);
    EXPECT_EQ (fetches, 1);
    ASSERT_NE (model.rowAt (100), nullptr);
    EXPECT_EQ (fetches, 2);
    EXPECT_EQ (model.rowAt (999)->title, "T999");
    EXPECT_EQ (fetches, 3);
    for (int page = 0; page < 10; ++page)   // walking the whole list stays bounded by the page cache
        model.rowAt (page * TrackTableModel::kPageSize);
    EXPECT_EQ (model.rowAt (1000), nullptr);
    EXPECT_EQ (model.rowAt (-1), nullptr);
    const int before = fetches;
    model.rowAt (950);   // recently used page is still cached
    EXPECT_EQ (fetches, before);
}

TEST (LibraryBrowserTest, HarmonicallyCompatibleKeysAreFlaggedAgainstTheReference)
{
    BrowserRig rig;
    auto& model = rig.browser.getModel();
    EXPECT_FALSE (model.isCompatible (*model.rowAt (0)));   // no reference yet
    model.setReferenceCamelot ("8A");
    EXPECT_TRUE (model.isCompatible (*model.rowAt (0)));    // same key
    EXPECT_TRUE (model.isCompatible (*model.rowAt (1)));    // 9A, one step on the wheel
    EXPECT_FALSE (model.isCompatible (*model.rowAt (2)));   // 3B
    EXPECT_FALSE (model.isCompatible (*model.rowAt (3)));   // unknown key
    model.setReferenceCamelot ({});
    EXPECT_FALSE (model.isCompatible (*model.rowAt (0)));
}

TEST (LibraryBrowserTest, ReferenceKeyProviderIsPolledByTheTick)
{
    BrowserRig rig;
    juce::String camelot = "9A";
    rig.browser.referenceKey = [&] { return camelot; };
    rig.browser.flushPending();
    EXPECT_TRUE (rig.browser.getModel().isCompatible (*rig.browser.getModel().rowAt (0)));   // 8A next to 9A
    camelot = "3B";
    rig.browser.flushPending();
    EXPECT_FALSE (rig.browser.getModel().isCompatible (*rig.browser.getModel().rowAt (0)));
}

TEST (LibraryBrowserTest, SidebarListsAllTracksPlaylistsWatchFoldersAndThePrepQueue)
{
    BrowserRig rig;
    auto& lib = rig.services->getLibrary();
    const auto sets = *lib.playlists().create ("Sets", std::nullopt, true);
    const auto warmup = *lib.playlists().create ("Warmup", sets);
    const auto peak = *lib.playlists().create ("Peak");
    lib.playlists().addTrack (peak, rig.ids[1]);
    lib.playlists().addTrack (warmup, rig.ids[0]);
    lib.watchFolders().add ("/music/house");
    rig.services->notifyLibraryChanged();
    rig.browser.flushPending();

    const auto& nodes = rig.browser.getSidebar().getNodes();
    ASSERT_EQ (nodes.size(), 4u);
    EXPECT_EQ (nodes[0].kind, SidebarNode::Kind::AllTracks);
    EXPECT_EQ (nodes[1].label, "Playlists");
    ASSERT_EQ (nodes[1].children.size(), 2u);
    EXPECT_EQ (nodes[1].children[0].label, "Sets");
    EXPECT_EQ (nodes[1].children[0].kind, SidebarNode::Kind::Folder);
    ASSERT_EQ (nodes[1].children[0].children.size(), 1u);
    EXPECT_EQ (nodes[1].children[0].children[0].label, "Warmup");
    EXPECT_EQ (nodes[1].children[1].label, "Peak");
    ASSERT_EQ (nodes[2].children.size(), 1u);
    EXPECT_EQ (nodes[2].children[0].path, "/music/house");
    EXPECT_EQ (nodes[2].children[0].label, "house");
    EXPECT_EQ (nodes[3].kind, SidebarNode::Kind::PrepQueue);
    EXPECT_FALSE (nodes[1].isSelectable());
    EXPECT_EQ (nodes[1].children[1].scopeQuery(), "playlist:#" + juce::String (peak));
    EXPECT_GE (rig.browser.getSidebar().getNumRows(), 8);   // groups are always expanded

    ASSERT_TRUE (rig.browser.getSidebar().select (SidebarNode::Kind::Playlist, peak));
    ASSERT_EQ (rig.browser.getModel().getNumRows(), 1);
    EXPECT_EQ (rig.title (0), "Bravo");
    ASSERT_TRUE (rig.browser.getSidebar().select (SidebarNode::Kind::Playlist, warmup));
    EXPECT_EQ (rig.title (0), "Alpha");
    rig.browser.setSearchText ("alpha");   // search applies inside the selected playlist
    EXPECT_EQ (rig.browser.getModel().getNumRows(), 1);
    rig.browser.setSearchText ("bravo");
    EXPECT_EQ (rig.browser.getModel().getNumRows(), 0);
    ASSERT_TRUE (rig.browser.getSidebar().select (SidebarNode::Kind::AllTracks));
    EXPECT_EQ (rig.browser.getModel().getNumRows(), 1);
}

TEST (LibraryBrowserTest, WatchFolderNodeScopesTheListToThatPath)
{
    BrowserRig rig;
    auto& lib = rig.services->getLibrary();
    addTrack (lib, { "InFolder", "A", 100.0, "1A", "", 0 }, "/music/house/in.wav");
    lib.watchFolders().add ("/music/house");
    rig.browser.refresh();
    ASSERT_TRUE (rig.browser.getSidebar().select (SidebarNode::Kind::WatchFolder, 0, "/music/house"));
    ASSERT_EQ (rig.browser.getModel().getNumRows(), 1);
    EXPECT_EQ (rig.title (0), "InFolder");
}

TEST (LibraryBrowserTest, PrepQueueNodeListsQueuedTracksAndPrepButtonQueuesSelection)
{
    BrowserRig rig;
    library::test::TempDir dir;   // real files so the fake analysers can read them
    for (const char* name : { "Real1", "Real2" })
        addTrack (rig.services->getLibrary(), { name, "Real", std::nullopt, "", "", 0 },
                  library::test::writeWav (dir.dir.getChildFile (juce::String (name) + ".wav"), 1.0).getFullPathName());
    rig.browser.setSearchText ("artist:real");
    rig.browser.getTable().selectRangeOfRows (0, 1);
    EXPECT_EQ (rig.browser.prepSelected(), 2);
    ASSERT_TRUE (rig.browser.getSidebar().select (SidebarNode::Kind::PrepQueue));
    EXPECT_EQ (rig.browser.getModel().getNumRows(), 2);
    EXPECT_EQ (rig.browser.getStatusLabel().getText(), "2 in the prep queue");
    ASSERT_TRUE (pumpUntil (*rig.services, [&]
    {
        for (const auto& e : rig.services->getPrepQueue().list())
            if (e.status != library::PrepStatus::Done)
                return false;
        return true;
    }));
    rig.browser.flushPending();
    ASSERT_TRUE (rig.browser.getSidebar().select (SidebarNode::Kind::AllTracks));
    rig.browser.setSearchText ("artist:real");
    EXPECT_EQ (rig.browser.getModel().cellText (0, 3), "128.0");   // analysis wrote bpm back and the table shows it
    EXPECT_EQ (rig.browser.getModel().cellText (0, 4), "8A");
}

TEST (LibraryBrowserTest, LoadButtonsAndActivationRequestTheRightDeck)
{
    BrowserRig rig;
    std::vector<std::pair<int, juce::String>> loads;
    rig.browser.onLoadRequested = [&] (int deck, const library::TrackRecord& t) { loads.emplace_back (deck, t.title); return juce::Result::ok(); };
    rig.browser.defaultDeck = [] { return 1; };

    rig.browser.getLoadButton (0).onClick();   // nothing selected: no load
    EXPECT_TRUE (loads.empty());
    rig.browser.getTable().selectRow (1);
    rig.browser.getLoadButton (1).onClick();
    rig.browser.getLoadButton (0).onClick();
    ASSERT_EQ (loads.size(), 2u);
    EXPECT_EQ (loads[0], std::make_pair (1, juce::String ("Bravo")));
    EXPECT_EQ (loads[1], std::make_pair (0, juce::String ("Bravo")));

    rig.browser.getModel().returnKeyPressed (2);   // Enter / double-click path
    ASSERT_EQ (loads.size(), 3u);
    EXPECT_EQ (loads[2], std::make_pair (1, juce::String ("Charlie")));
}

TEST (LibraryBrowserTest, FailedLoadsShowTheReasonInTheStatusLine)
{
    BrowserRig rig;
    rig.browser.onLoadRequested = [] (int, const library::TrackRecord&) { return juce::Result::fail ("file is missing"); };
    rig.browser.getTable().selectRow (0);
    rig.browser.getLoadButton (0).onClick();
    EXPECT_EQ (rig.browser.getStatusLabel().getText(), "Couldn't load " + juce::File (rig.browser.getModel().rowAt (0)->path).getFileName() + ": file is missing");
    rig.browser.onLoadRequested = [] (int, const library::TrackRecord&) { return juce::Result::ok(); };
    rig.browser.getLoadButton (0).onClick();
    EXPECT_EQ (rig.browser.getStatusLabel().getText().substring (0, 6), "Loaded");
    EXPECT_TRUE (rig.browser.requestLoad (0, *rig.browser.getModel().rowAt (0)).wasOk());
}

TEST (LibraryBrowserTest, SearchBoxTextIsVerticallyCentred)
{
    BrowserRig rig;
    rig.browser.setSize (1280, 300);
    const auto indent = rig.browser.getSearchBox().getTopIndent();
    EXPECT_GT (indent, 2);
    EXPECT_NEAR (indent * 2 + theme::font (theme::typeBody()).getHeight(), static_cast<float> (rig.browser.getSearchBox().getHeight()), 2.0F);
}

TEST (LibraryBrowserTest, WatchFolderScopeIsAnchoredAndDoesNotMatchSiblings)
{
    BrowserRig rig;
    auto& lib = rig.services->getLibrary();
    addTrack (lib, { "In", "A", 100.0, "1A", "", 0 }, "/music/house/in.wav");
    addTrack (lib, { "Deep", "A", 100.0, "1A", "", 0 }, "/music/house/sub/deep.wav");
    addTrack (lib, { "Sibling", "A", 100.0, "1A", "", 0 }, "/music/house2/sib.wav");
    addTrack (lib, { "Elsewhere", "A", 100.0, "1A", "", 0 }, "/x/music/house/e.wav");
    addTrack (lib, { "Pct", "A", 100.0, "1A", "", 0 }, "/music/100%_x/p.wav");
    addTrack (lib, { "PctOther", "A", 100.0, "1A", "", 0 }, "/music/100abx/p.wav");
    lib.watchFolders().add ("/music/house");
    lib.watchFolders().add ("/music/100%_x");
    rig.browser.refresh();
    ASSERT_TRUE (rig.browser.getSidebar().select (SidebarNode::Kind::WatchFolder, 0, "/music/house"));
    ASSERT_EQ (rig.browser.getModel().getNumRows(), 2);
    juce::StringArray titles { rig.title (0), rig.title (1) };
    EXPECT_TRUE (titles.contains ("In") && titles.contains ("Deep"));
    ASSERT_TRUE (rig.browser.getSidebar().select (SidebarNode::Kind::WatchFolder, 0, "/music/100%_x"));
    ASSERT_EQ (rig.browser.getModel().getNumRows(), 1);   // LIKE wildcards in the folder name are escaped
    EXPECT_EQ (rig.title (0), "Pct");
    ASSERT_TRUE (rig.browser.getSidebar().select (SidebarNode::Kind::AllTracks));
    rig.browser.setSearchText ("folder:/music/house/ -title:deep");   // trailing slash accepted; composes with other terms
    ASSERT_EQ (rig.browser.getModel().getNumRows(), 1);
}

TEST (LibraryBrowserTest, WatchFoldersWithTheSameLeafNameGetDistinctLabels)
{
    BrowserRig rig;
    auto& lib = rig.services->getLibrary();
    lib.watchFolders().add ("/a/music");
    lib.watchFolders().add ("/b/music");
    lib.watchFolders().add ("/c/other");
    const auto nodes = buildSidebar (lib);
    const auto& kids = nodes[2].children;
    ASSERT_EQ (kids.size(), 3u);
    EXPECT_NE (kids[0].label, kids[1].label);
    EXPECT_NE (kids[0].label.indexOf ("/a"), -1);
    EXPECT_EQ (kids[2].label, "other");
}

TEST (LibraryBrowserTest, RowsAreDraggableAndDecksAcceptTheDrop)
{
    BrowserRig rig;
    juce::SparseSet<int> rows;
    rows.addRange ({ 2, 3 });
    const auto description = rig.browser.getModel().getDragSourceDescription (rows);
    ASSERT_TRUE (trackdrag::parse (description).has_value());
    EXPECT_EQ (*trackdrag::parse (description), rig.ids[2]);
    EXPECT_TRUE (rig.browser.getModel().getDragSourceDescription ({}).isVoid());
    EXPECT_FALSE (trackdrag::parse ("not a track").has_value());

    DjEngine engine (2);
    ParameterRouter router;
    DeckWidget deck ({ engine, router, 0 });
    const juce::DragAndDropTarget::SourceDetails details (description, nullptr, {});
    EXPECT_FALSE (deck.isInterestedInDragSource (details));   // nobody listens yet
    std::int64_t dropped = 0;
    deck.onTrackDropped = [&] (std::int64_t id) { dropped = id; };
    EXPECT_TRUE (deck.isInterestedInDragSource (details));
    EXPECT_FALSE (deck.isInterestedInDragSource ({ "x", nullptr, {} }));
    deck.itemDragEnter (details);
    deck.itemDropped (details);
    EXPECT_EQ (dropped, rig.ids[2]);
}

TEST (LibraryBrowserTest, ImportsRekordboxXmlThroughTheServices)
{
    BrowserRig rig;
    int changes = 0;
    struct Counter : LibraryServices::Listener
    {
        explicit Counter (int& n) : count (n) {}
        void libraryChanged() override { ++count; }
        int& count;
    } counter (changes);
    rig.services->addListener (&counter);

    ASSERT_TRUE (rig.browser.importFrom (importFixture ("rekordbox_small.xml")).wasOk());
    EXPECT_EQ (changes, 1);
    EXPECT_EQ (rig.browser.getStatusLabel().getText().substring (0, 8), "Imported");
    rig.browser.flushPending();
    EXPECT_GT (rig.browser.getModel().getNumRows(), 4);
    EXPECT_GE (rig.browser.getSidebar().getNodes()[1].children.size(), 1u);   // imported playlists show up

    EXPECT_TRUE (rig.browser.importFrom (importFixture ("does_not_exist.xml")).failed());
    EXPECT_NE (rig.browser.getStatusLabel().getText().indexOf ("failed"), -1);
    rig.services->removeListener (&counter);
}

TEST (LibraryBrowserTest, BrowserAndTableRenderWithGlassTheme)
{
    GlassLookAndFeel lnf;
    juce::LookAndFeel::setDefaultLookAndFeel (&lnf);
    {
        BrowserRig rig;
        rig.browser.getModel().setReferenceCamelot ("8A");
        rig.browser.getTable().selectRow (0);
        rig.browser.setBounds (0, 0, 1200, 300);
        const auto image = rig.browser.createComponentSnapshot (rig.browser.getLocalBounds());
        EXPECT_GT (distinctPixels (image), 20);
        ThemeRegistry registry;
        registry.loadBuiltIns();
        for (const auto* id : { "openmixer.daylight", "openmixer.dark" })
        {
            ASSERT_TRUE (registry.select (id));
            EXPECT_GT (distinctPixels (rig.browser.createComponentSnapshot (rig.browser.getLocalBounds())), 20);
        }
    }
    juce::LookAndFeel::setDefaultLookAndFeel (nullptr);
}

TEST (LibraryBrowserTest, EveryToolbarButtonLabelFitsAndNothingOverlaps)
{
    BrowserRig rig;
    rig.browser.setSize (1280, 300);
    std::vector<juce::Component*> parts { &rig.browser.getSearchBox(), &rig.browser.getLoadButton (0), &rig.browser.getLoadButton (1),
                                          &rig.browser.getPrepButton(), &rig.browser.getImportButton() };
    for (size_t i = 0; i < parts.size(); ++i)
    {
        EXPECT_GT (parts[i]->getWidth(), 0);
        for (size_t j = i + 1; j < parts.size(); ++j)
            EXPECT_FALSE (parts[i]->getBounds().intersects (parts[j]->getBounds())) << i << " vs " << j;
    }
    EXPECT_GE (rig.browser.getSearchBox().getWidth(), 160);
    juce::GlyphArrangement glyphs;
    glyphs.addLineOfText (theme::font (theme::typeBody()), "Import...", 0.0F, 0.0F);
    EXPECT_LE (glyphs.getBoundingBox (0, -1, true).getWidth(), static_cast<float> (rig.browser.getImportButton().getWidth() - 12));
}

TEST (LibraryServicesTest, InMemoryLibraryHasNoWatchFoldersAndPumpsThePrepQueue)
{
    auto services = makeServices();
    EXPECT_EQ (services->getWatchFolders(), nullptr);
    library::test::TempDir dir;
    const auto id = addTrack (services->getLibrary(), { "T", "A", std::nullopt, "", "", 0 },
                              library::test::writeWav (dir.dir.getChildFile ("t.wav"), 1.0).getFullPathName());
    std::vector<library::PrepEntry> events;
    struct Rec : LibraryServices::Listener
    {
        explicit Rec (std::vector<library::PrepEntry>& e) : out (e) {}
        void prepEntryChanged (const library::PrepEntry& e) override { out.push_back (e); }
        std::vector<library::PrepEntry>& out;
    } rec (events);
    services->addListener (&rec);
    ASSERT_TRUE (services->getPrepQueue().enqueue (id).wasOk());
    ASSERT_TRUE (pumpUntil (*services, [&] { return ! events.empty() && events.back().status == library::PrepStatus::Done; }));
    services->removeListener (&rec);
}

TEST (LibraryServicesTest, FileLibraryScansWatchFoldersAndNotifiesTheMessageThread)
{
    library::test::TempDir dir;
    library::test::writeWav (dir.dir.getChildFile ("music/a.wav"), 1.0);
    auto options = memoryOptions();
    options.dbPath = dir.dir.getChildFile ("db/library.db").getFullPathName();
    options.scanOnStart = false;
    auto services = makeServices (std::move (options));
    ASSERT_NE (services, nullptr);
    ASSERT_NE (services->getWatchFolders(), nullptr);
    EXPECT_TRUE (dir.dir.getChildFile ("db").isDirectory());   // parent folder is created

    int changes = 0;
    struct Counter : LibraryServices::Listener
    {
        explicit Counter (int& n) : count (n) {}
        void libraryChanged() override { ++count; }
        int& count;
    } counter (changes);
    services->addListener (&counter);
    ASSERT_TRUE (services->getWatchFolders()->add (dir.dir.getChildFile ("music")).wasOk());
    ASSERT_TRUE (pumpUntil (*services, [&] { return changes > 0; }));
    EXPECT_EQ (services->getLibrary().tracks().count(), 1);
    EXPECT_EQ (buildSidebar (services->getLibrary())[2].children.size(), 1u);
    services->removeListener (&counter);
}

#if ! JUCE_WINDOWS
TEST (LibraryServicesTest, DefaultOptionsHonourTheLibraryDbEnvironmentVariable)
{
    setenv ("OPENMIXER_LIBRARY_DB", "/tmp/om_env_test/library.db", 1);
    EXPECT_EQ (LibraryServices::defaultOptions().dbPath, "/tmp/om_env_test/library.db");
    unsetenv ("OPENMIXER_LIBRARY_DB");
    EXPECT_TRUE (LibraryServices::defaultOptions().dbPath.endsWith ("OpenMixer/library.db"));
}
#endif

TEST (LibraryServicesTest, UnopenableDatabaseReportsAnError)
{
    auto options = memoryOptions();
    options.dbPath = "/proc/does/not/exist/library.db";
    juce::String error;
    EXPECT_EQ (LibraryServices::create (std::move (options), &error), nullptr);
    EXPECT_TRUE (error.isNotEmpty());
}

namespace
{
// Runs queued callAsync messages: a stop request queued behind them ends the loop.
void drainMessages()
{
    juce::MessageManager::callAsync ([] { juce::MessageManager::getInstance()->stopDispatchLoop(); });
    juce::MessageManager::getInstance()->runDispatchLoop();
}
}

TEST (LibraryServicesTest, ScanThreadErrorsAreDeliveredOnTheMessageThread)
{
    library::test::TempDir dir;
    std::vector<std::pair<juce::String, bool>> errors;   // message, delivered on the test (message) thread
    const auto messageThread = std::this_thread::get_id();
    auto options = memoryOptions();
    options.dbPath = dir.dir.getChildFile("library.db").getFullPathName();
    options.scanOnStart = false;
    options.onError = [&] (const juce::String& m) { errors.emplace_back (m, std::this_thread::get_id() == messageThread); };
    auto services = makeServices (std::move (options));
    ASSERT_NE (services, nullptr);
    ASSERT_TRUE (services->getLibrary().database().exec ("INSERT INTO schema_version (version, name, applied_at) VALUES (99, 'future', 0);").wasOk());

    services->getWatchFolders()->triggerScan();   // the scan thread cannot open the "newer" database
    ASSERT_TRUE (services->getWatchFolders()->waitForScans (1, 8000));
    for (int i = 0; i < 100 && errors.empty(); ++i)
    {
        juce::Thread::sleep (10);
        drainMessages();
    }
    ASSERT_FALSE (errors.empty());
    EXPECT_TRUE (errors[0].second);
    EXPECT_TRUE (errors[0].first.contains ("newer"));

    services->getWatchFolders()->triggerScan();   // an error still queued when the services die must be dropped safely
    services->getWatchFolders()->waitForScans (2, 8000);
    services.reset();
    drainMessages();
}
