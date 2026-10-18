// OpenMixer TrackLoaderTest - load-to-deck: file into the deck, metadata, cached or queued analysis (needs GUI init from TestMain)
#include <JuceHeader.h>
#include "LibraryUiTestUtil.h"
#include "analysis/ClipSource.h"
#include "controllers/ParameterRouter.h"
#include "ui/MainView.h"
#include "ui/TrackLoader.h"

#include <gtest/gtest.h>

using namespace openmixer;
using namespace openmixer::ui;
using namespace libui;

namespace
{
struct LoaderRig
{
    LoaderRig() : services (makeServices()), engine (2), view (engine, router), loader (engine, view, *services)
    {
        engine.prepare (48000.0, 256);
        for (int i = 0; i < 2; ++i)
            router.registerTarget ("deck." + juce::String (i), &engine.getDeckControl (i));
        view.setSize (1280, 800);
    }

    // A real WAV in the temp dir, registered with the content hash the analysis cache will use.
    std::int64_t addFile (const char* name, const char* camelot = "8A", bool exists = true)
    {
        const auto file = dir.dir.getChildFile (name);
        if (exists)
            library::test::writeWav (file, 2.0, {}, 220.0F + static_cast<float> (counter++ * 40));
        library::TrackRecord t;
        t.path = file.getFullPathName();
        t.contentHash = exists ? analysis::hashFileContent (file) : juce::String();
        t.title = name;
        t.artist = "Artist";
        t.camelot = camelot;
        t.key = "Am";
        return services->getLibrary().tracks().insert (t).value_or (0);
    }

    bool waitForWaveform (int deck)
    {
        return pumpUntil (*services, [&] { return view.getDeck (deck)->getOverview().getWaveform() != nullptr; });
    }

    library::test::TempDir dir;
    std::unique_ptr<LibraryServices> services;
    DjEngine engine;
    ParameterRouter router;
    MainView view;
    TrackLoader loader;
    int counter = 0;
};
}

TEST (TrackLoaderTest, LoadsTheFileIntoTheDeckAndShowsMetadata)
{
    LoaderRig rig;
    const auto id = rig.addFile ("one.wav");
    ASSERT_TRUE (rig.loader.load (0, id).wasOk());
    EXPECT_TRUE (rig.engine.getDeck (0).isLoaded());
    EXPECT_FALSE (rig.engine.getDeck (1).isLoaded());
    rig.view.getDeck (0)->refresh();
    EXPECT_EQ (rig.view.getDeck (0)->getHeader().getTitleLabel().getText(), "one.wav");
    EXPECT_EQ (rig.view.getDeck (0)->getHeader().getArtistLabel().getText(), "Artist");
    EXPECT_NE (rig.view.getDeck (0)->getHeader().getKeyLabel().getText().indexOf ("8A"), -1);
    ASSERT_TRUE (rig.loader.loadedTrack (0).has_value());
    EXPECT_EQ (rig.loader.loadedTrack (0)->id, id);
    EXPECT_FALSE (rig.loader.loadedTrack (1).has_value());
}

TEST (TrackLoaderTest, AnUnanalysedTrackIsQueuedWithPriorityAndFilledInWhenDone)
{
    LoaderRig rig;
    const auto first = rig.addFile ("a.wav");
    const auto second = rig.addFile ("b.wav");
    ASSERT_TRUE (rig.services->getPrepQueue().enqueue (first).wasOk());   // a normal item already waiting
    ASSERT_TRUE (rig.loader.load (0, second).wasOk());
    const auto entries = rig.services->getPrepQueue().list();
    ASSERT_GE (entries.size(), 2u);
    EXPECT_EQ (rig.view.getDeck (0)->getOverview().getWaveform(), nullptr);   // not there yet

    ASSERT_TRUE (rig.waitForWaveform (0));
    EXPECT_NE (rig.view.getDeck (0)->getZoom().getWaveform(), nullptr);
    const auto grid = rig.engine.getDeckControl (0).getGrid();
    ASSERT_NE (grid, nullptr);
    EXPECT_NEAR (grid->getBpm(), 128.0, 1e-6);
    rig.view.getDeck (0)->refresh();
    EXPECT_EQ (rig.view.getDeck (0)->getHeader().getBpmLabel().getText(), "128.0");   // from the new grid / written-back bpm
    EXPECT_EQ (rig.loader.loadedTrack (0)->bpm.value_or (0.0), 128.0);
}

TEST (TrackLoaderTest, AnAlreadyAnalysedTrackLoadsImmediatelyWithoutQueueing)
{
    LoaderRig rig;
    const auto id = rig.addFile ("cached.wav");
    ASSERT_TRUE (rig.loader.load (0, id).wasOk());
    ASSERT_TRUE (rig.waitForWaveform (0));
    ASSERT_TRUE (pumpUntil (*rig.services, [&] { return rig.services->getPrepQueue().list().front().status == library::PrepStatus::Done; }));

    ASSERT_TRUE (rig.loader.load (1, id).wasOk());   // same track on the other deck: results come from the cache
    EXPECT_NE (rig.view.getDeck (1)->getOverview().getWaveform(), nullptr);
    ASSERT_NE (rig.engine.getDeckControl (1).getGrid(), nullptr);
    EXPECT_EQ (rig.services->getPrepQueue().list().front().status, library::PrepStatus::Done);   // not re-queued
}

TEST (TrackLoaderTest, LoadingAnotherTrackClearsTheOldWaveformAndGrid)
{
    LoaderRig rig;
    const auto a = rig.addFile ("x.wav");
    ASSERT_TRUE (rig.loader.load (0, a).wasOk());
    ASSERT_TRUE (rig.waitForWaveform (0));
    rig.services->getPrepQueue().cancelAll();
    const auto b = rig.addFile ("y.wav");
    rig.services->getPrepQueue().cancelAll();
    // y is not analysed: its load must not show x's data.
    auto& svc = *rig.services;
    ASSERT_TRUE (rig.loader.load (0, b).wasOk());
    EXPECT_EQ (rig.view.getDeck (0)->getOverview().getWaveform(), nullptr);
    EXPECT_EQ (rig.engine.getDeckControl (0).getGrid(), nullptr);
    ASSERT_TRUE (rig.waitForWaveform (0));   // and then it does arrive
    (void) svc;
}

TEST (TrackLoaderTest, MissingFilesAndBadDecksFailWithoutTouchingTheDeck)
{
    LoaderRig rig;
    const auto missing = rig.addFile ("gone.wav", "8A", false);
    const auto r = rig.loader.load (0, missing);
    EXPECT_TRUE (r.failed());
    EXPECT_NE (r.getErrorMessage().indexOf ("missing"), -1);
    EXPECT_FALSE (rig.engine.getDeck (0).isLoaded());
    EXPECT_TRUE (rig.loader.load (5, rig.addFile ("ok.wav")).failed());
    EXPECT_TRUE (rig.loader.load (0, 99999).failed());   // unknown track id
    EXPECT_TRUE (rig.services->getPrepQueue().list().empty());
}

TEST (TrackLoaderTest, SuggestedDeckPrefersAnEmptyIdleDeck)
{
    LoaderRig rig;
    EXPECT_EQ (rig.loader.suggestedDeck(), 0);
    ASSERT_TRUE (rig.loader.load (0, rig.addFile ("p.wav", "8A")).wasOk());
    EXPECT_EQ (rig.loader.suggestedDeck(), 1);
    ASSERT_TRUE (rig.loader.load (1, rig.addFile ("q.wav", "9A")).wasOk());
    EXPECT_EQ (rig.loader.suggestedDeck(), 0);   // both loaded and idle: the first
    rig.engine.getDeck (0).setPlaying (true);
    EXPECT_EQ (rig.loader.suggestedDeck(), 1);
}

TEST (TrackLoaderTest, ReferenceKeyFollowsThePlayingDeck)
{
    LoaderRig rig;
    EXPECT_EQ (rig.loader.referenceCamelot(), "");
    ASSERT_TRUE (rig.loader.load (0, rig.addFile ("p.wav", "8A")).wasOk());
    ASSERT_TRUE (rig.loader.load (1, rig.addFile ("q.wav", "9A")).wasOk());
    EXPECT_EQ (rig.loader.referenceCamelot(), "8A");   // nothing playing: first loaded
    rig.engine.getDeck (1).setPlaying (true);
    EXPECT_EQ (rig.loader.referenceCamelot(), "9A");
}
