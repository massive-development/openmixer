// OpenMixer LibraryPrepQueueTest - persisted prep queue feeding the analysis service
#include <JuceHeader.h>
#include "LibraryIngestTestUtil.h"
#include "AnalysisTestSignals.h"
#include "analysis/BpmEstimator.h"
#include "analysis/KeyEstimator.h"
#include "library/PrepQueue.h"

#include <gtest/gtest.h>

#include <atomic>
#include <thread>

using namespace openmixer::library;
using namespace openmixer::analysis;

namespace
{
constexpr int kWaitMs = 10000;

// Returns a fixed payload and records the clip length (= track id * 1000) in run order.
struct FakeAnalyzer final : Analyzer
{
    FakeAnalyzer (juce::String analyzerId, juce::String result) : name (std::move (analyzerId)), payload (std::move (result)) {}
    juce::String getId() const override { return name; }
    int getVersion() const override { return 1; }
    std::optional<juce::String> analyze (const AudioClip& clip, AnalysisContext&) const override
    {
        const auto now = running.fetch_add (1) + 1;
        peak.store (std::max (peak.load(), now));
        if (name == "bpm")
        {
            std::lock_guard<std::mutex> lock (mutex);
            order.push_back (static_cast<std::int64_t> (clip.samples.size()) / 1000);
        }
        if (delayMs > 0)
            std::this_thread::sleep_for (std::chrono::milliseconds (delayMs));
        running.fetch_sub (1);
        return payload;
    }
    juce::String name, payload;
    int delayMs = 0;
    mutable std::mutex mutex;
    mutable std::vector<std::int64_t> order;
    mutable std::atomic<int> running { 0 }, peak { 0 };
};

struct GateAnalyzer final : Analyzer   // spins until cancelled
{
    juce::String getId() const override { return "waveform"; }
    int getVersion() const override { return 1; }
    std::optional<juce::String> analyze (const AudioClip&, AnalysisContext& context) const override
    {
        started.store (true);
        while (! context.isCancelled())
            std::this_thread::sleep_for (std::chrono::milliseconds (1));
        return std::nullopt;
    }
    mutable std::atomic<bool> started { false };
};

struct NullAnalyzer final : Analyzer   // always fails
{
    juce::String getId() const override { return "structure"; }
    int getVersion() const override { return 1; }
    std::optional<juce::String> analyze (const AudioClip&, AnalysisContext&) const override { return std::nullopt; }
};

juce::String bpmJson (double bpm) { BpmResult r; r.bpm = bpm; return r.toJson(); }
juce::String keyJson (int tonic, bool minor, bool tonal = true)
{
    KeyResult r;
    r.key = { tonic, minor };
    r.tonal = tonal;
    return r.toJson();
}

struct Fixture
{
    std::shared_ptr<FakeAnalyzer> bpm = std::make_shared<FakeAnalyzer> ("bpm", bpmJson (128.0));
    std::shared_ptr<FakeAnalyzer> key = std::make_shared<FakeAnalyzer> ("key", keyJson (9, true));   // Am = 8A
    AnalyzerRegistry registry;
    std::unique_ptr<Library> lib;
    std::unique_ptr<AnalysisService> service;
    std::vector<PrepEntry> events;
    std::vector<std::thread::id> eventThreads;
    std::vector<juce::String> errors;

    explicit Fixture (int workers = 2, const juce::String& dbPath = ":memory:")
    {
        registry.add (bpm);
        registry.add (key);
        registry.add (std::make_shared<FakeAnalyzer> ("waveform", "{}"));
        registry.add (std::make_shared<FakeAnalyzer> ("structure", "{}"));
        lib = Library::open (dbPath);
        service = std::make_unique<AnalysisService> (registry, nullptr, workers);
    }

    std::int64_t addTrack (const juce::String& path = {})
    {
        TrackRecord t;
        t.path = path.isEmpty() ? "/music/t" + juce::String (++counter) + ".wav" : path;
        return lib->tracks().insert (t).value_or (0);
    }

    PrepQueue::Options options (int maxInFlight = 2)
    {
        PrepQueue::Options o;
        o.maxInFlight = maxInFlight;
        // clip length encodes the track id so analysers can report run order
        o.sourceFactory = [] (const TrackRecord& t)
        {
            AudioClip clip;
            clip.sampleRate = 44100.0;
            clip.samples.assign (static_cast<size_t> (t.id * 1000), 0.0f);
            return std::make_unique<MemoryClipSource> (std::move (clip), t.path);
        };
        o.observer = [this] (const PrepEntry& e) { events.push_back (e); eventThreads.push_back (std::this_thread::get_id()); };
        o.onError = [this] (const juce::String& m) { errors.push_back (m); };
        return o;
    }

    void replace (std::shared_ptr<const Analyzer> analyzer)
    {
        registry.remove (analyzer->getId());
        registry.add (analyzer);
    }

    int counter = 0;
};

PrepStatus statusOf (Fixture& f, std::int64_t id)
{
    return f.lib->prepQueue().find (id).value().status;
}
} // namespace

TEST (PrepQueueDao, EnqueueKeepsOrderAndMoveRewritesPositions)
{
    Fixture f;
    const auto a = f.addTrack(), b = f.addTrack(), c = f.addTrack();
    auto& dao = f.lib->prepQueue();
    for (auto id : { a, b, c })
        ASSERT_TRUE (dao.enqueue (id).wasOk());

    ASSERT_TRUE (dao.move (c, 0).wasOk());
    const auto all = dao.all();
    ASSERT_EQ (all.size(), 3u);
    EXPECT_EQ (all[0].trackId, c);
    EXPECT_EQ (all[1].trackId, a);
    EXPECT_EQ (all[2].trackId, b);
    EXPECT_EQ (all[2].position, 2);
    EXPECT_TRUE (dao.move (a, 99).wasOk());   // clamps to the end
    EXPECT_EQ (dao.all().back().trackId, a);
    EXPECT_TRUE (dao.move (9999, 0).failed());
}

TEST (PrepQueueDao, PrioritiseJumpsAheadAndLatestWins)
{
    Fixture f;
    const auto a = f.addTrack(), b = f.addTrack(), c = f.addTrack();
    auto& dao = f.lib->prepQueue();
    for (auto id : { a, b, c })
        dao.enqueue (id);
    dao.prioritise (b);
    EXPECT_EQ (dao.nextQueued()->trackId, b);
    dao.prioritise (c);
    EXPECT_EQ (dao.nextQueued()->trackId, c);
    EXPECT_EQ (dao.all()[0].trackId, a);   // user order is untouched
}

TEST (PrepQueueDao, RequeueResetsFinishedButNotRunning)
{
    Fixture f;
    const auto a = f.addTrack(), b = f.addTrack();
    auto& dao = f.lib->prepQueue();
    dao.enqueue (a);
    dao.enqueue (b);
    dao.setStatus (a, PrepStatus::Failed, "boom");
    dao.setStatus (b, PrepStatus::Running);
    dao.enqueue (a);
    dao.enqueue (b);
    EXPECT_EQ (dao.find (a)->status, PrepStatus::Queued);
    EXPECT_TRUE (dao.find (a)->error.isEmpty());
    EXPECT_EQ (dao.find (a)->position, 2);
    EXPECT_EQ (dao.find (b)->status, PrepStatus::Running);
}

TEST (PrepQueueDao, MissingTrackFailsAndDeletedTrackCascades)
{
    Fixture f;
    const auto a = f.addTrack();
    auto& dao = f.lib->prepQueue();
    EXPECT_TRUE (dao.enqueue (12345).failed());
    dao.enqueue (a);
    f.lib->tracks().remove (a);
    EXPECT_TRUE (dao.all().empty());
}

TEST (PrepQueueDao, ResetRunningAndClearFinished)
{
    Fixture f;
    const auto a = f.addTrack(), b = f.addTrack(), c = f.addTrack();
    auto& dao = f.lib->prepQueue();
    for (auto id : { a, b, c })
        dao.enqueue (id);
    dao.setStatus (a, PrepStatus::Running);
    dao.setStatus (b, PrepStatus::Done);
    dao.resetRunning();
    EXPECT_EQ (dao.find (a)->status, PrepStatus::Queued);
    dao.clearFinished();
    EXPECT_FALSE (dao.find (b).has_value());
    EXPECT_TRUE (dao.find (c).has_value());
}

TEST (PrepQueueDao, FailedReadsAreReportedNotEmpty)
{
    Fixture f;
    f.lib->database().exec ("DROP TABLE prep_queue;");
    juce::Result status = juce::Result::ok();
    EXPECT_TRUE (f.lib->prepQueue().all (&status).empty());
    EXPECT_TRUE (status.failed());
    status = juce::Result::ok();
    EXPECT_FALSE (f.lib->prepQueue().nextQueued (&status).has_value());
    EXPECT_TRUE (status.failed());
    status = juce::Result::ok();
    EXPECT_FALSE (f.lib->prepQueue().find (1, &status).has_value());
    EXPECT_TRUE (status.failed());
}

TEST (PrepQueue, ProcessesAllTracksAndWritesBackBpmAndKey)
{
    Fixture f;
    const auto a = f.addTrack(), b = f.addTrack();
    PrepQueue q (*f.lib, *f.service, f.options());
    q.enqueue (a);
    q.enqueue (b);
    ASSERT_TRUE (q.pumpUntilIdle (kWaitMs));

    for (auto id : { a, b })
    {
        const auto t = f.lib->tracks().findById (id);
        ASSERT_TRUE (t.has_value());
        EXPECT_DOUBLE_EQ (t->bpm.value_or (0), 128.0);
        EXPECT_EQ (t->key, "Am");
        EXPECT_EQ (t->camelot, "8A");
        EXPECT_EQ (statusOf (f, id), PrepStatus::Done);
    }
    for (const auto& e : q.list())
        EXPECT_FLOAT_EQ (e.progress, 1.0f);
    EXPECT_TRUE (f.errors.empty());
}

TEST (PrepQueue, ObserverFiresOnTheOwnerThreadWithStatusChanges)
{
    Fixture f;
    const auto a = f.addTrack();
    PrepQueue q (*f.lib, *f.service, f.options());
    q.enqueue (a);
    ASSERT_TRUE (q.pumpUntilIdle (kWaitMs));

    ASSERT_GE (f.events.size(), 2u);
    EXPECT_EQ (f.events.front().status, PrepStatus::Queued);
    EXPECT_EQ (f.events.back().status, PrepStatus::Done);
    for (auto id : f.eventThreads)
        EXPECT_EQ (id, std::this_thread::get_id());
}

TEST (PrepQueue, UrgentTrackRunsBeforeEarlierQueuedOnes)
{
    Fixture f (1);
    const auto a = f.addTrack(), b = f.addTrack(), c = f.addTrack(), d = f.addTrack();
    PrepQueue q (*f.lib, *f.service, f.options (1));
    q.enqueue (a);
    q.enqueue (b);
    q.enqueue (c);
    q.pump();            // a is in flight
    q.enqueue (d, true); // jumps ahead of b and c
    ASSERT_TRUE (q.pumpUntilIdle (kWaitMs));

    const std::vector<std::int64_t> expected { a, d, b, c };
    EXPECT_EQ (f.bpm->order, expected);
}

TEST (PrepQueue, MoveChangesProcessingOrderOfWaitingItems)
{
    Fixture f (1);
    const auto a = f.addTrack(), b = f.addTrack(), c = f.addTrack();
    PrepQueue q (*f.lib, *f.service, f.options (1));
    q.enqueue (a);
    q.enqueue (b);
    q.enqueue (c);
    q.pump();
    q.move (c, 0);
    ASSERT_TRUE (q.pumpUntilIdle (kWaitMs));
    const std::vector<std::int64_t> expected { a, c, b };
    EXPECT_EQ (f.bpm->order, expected);
}

TEST (PrepQueue, NeverSubmitsMoreThanMaxInFlight)
{
    Fixture f (4);
    f.bpm->delayMs = 20;
    PrepQueue q (*f.lib, *f.service, f.options (2));
    for (int i = 0; i < 6; ++i)
        q.enqueue (f.addTrack());
    q.pump();
    EXPECT_LE (f.service->getQueuedCount(), 2u);
    ASSERT_TRUE (q.pumpUntilIdle (kWaitMs));
    EXPECT_LE (f.bpm->peak.load(), 2);
    EXPECT_GE (f.bpm->peak.load(), 1);
    EXPECT_EQ (f.bpm->order.size(), 6u);
}

TEST (PrepQueue, CancelQueuedItemSkipsIt)
{
    Fixture f (1);
    const auto a = f.addTrack(), b = f.addTrack();
    PrepQueue q (*f.lib, *f.service, f.options (1));
    q.enqueue (a);
    q.enqueue (b);
    q.pump();
    q.cancel (b);
    ASSERT_TRUE (q.pumpUntilIdle (kWaitMs));
    EXPECT_EQ (statusOf (f, b), PrepStatus::Cancelled);
    EXPECT_FALSE (f.lib->tracks().findById (b)->bpm.has_value());
    EXPECT_EQ (f.bpm->order.size(), 1u);
}

TEST (PrepQueue, CancelRunningJobEndsCancelledAndWritesNothing)
{
    Fixture f (1);
    auto gate = std::make_shared<GateAnalyzer>();
    f.replace (gate);
    const auto a = f.addTrack();
    PrepQueue q (*f.lib, *f.service, f.options (1));
    q.enqueue (a);
    q.pump();
    while (! gate->started.load())
        std::this_thread::sleep_for (std::chrono::milliseconds (1));

    q.cancel (a);
    ASSERT_TRUE (q.pumpUntilIdle (kWaitMs));
    EXPECT_EQ (statusOf (f, a), PrepStatus::Cancelled);
    EXPECT_FALSE (f.lib->tracks().findById (a)->bpm.has_value());
}

TEST (PrepQueue, CancelAllStopsEverything)
{
    Fixture f (1);
    f.replace (std::make_shared<GateAnalyzer>());
    const auto a = f.addTrack(), b = f.addTrack();
    PrepQueue q (*f.lib, *f.service, f.options (1));
    q.enqueue (a);
    q.enqueue (b);
    q.pump();
    q.cancelAll();
    ASSERT_TRUE (q.pumpUntilIdle (kWaitMs));
    EXPECT_EQ (statusOf (f, a), PrepStatus::Cancelled);
    EXPECT_EQ (statusOf (f, b), PrepStatus::Cancelled);
}

TEST (PrepQueue, AnalyserFailureMarksItemFailedAndQueueContinues)
{
    Fixture f (1);
    f.replace (std::make_shared<NullAnalyzer>());
    const auto a = f.addTrack();
    PrepQueue q (*f.lib, *f.service, f.options (1));
    q.enqueue (a);
    ASSERT_TRUE (q.pumpUntilIdle (kWaitMs));
    EXPECT_EQ (statusOf (f, a), PrepStatus::Failed);
    EXPECT_FALSE (f.lib->prepQueue().find (a)->error.isEmpty());
}

TEST (PrepQueue, UnreadableFileFailsWithDefaultSourceFactory)
{
    Fixture f (1);
    const auto a = f.addTrack ("/nonexistent/none.wav");
    auto o = f.options (1);
    o.sourceFactory = nullptr;
    PrepQueue q (*f.lib, *f.service, std::move (o));
    q.enqueue (a);
    ASSERT_TRUE (q.pumpUntilIdle (kWaitMs));
    EXPECT_EQ (statusOf (f, a), PrepStatus::Failed);
}

TEST (PrepQueue, AtonalKeyAndUnusableBpmLeaveStoredValuesAlone)
{
    Fixture f (1);
    f.replace (std::make_shared<FakeAnalyzer> ("bpm", "{}"));   // unusable payload
    f.replace (std::make_shared<FakeAnalyzer> ("key", keyJson (0, false, false)));
    const auto a = f.addTrack();
    auto t = *f.lib->tracks().findById (a);
    t.key = "Dm";
    t.camelot = "7A";
    f.lib->tracks().update (t);

    PrepQueue q (*f.lib, *f.service, f.options (1));
    q.enqueue (a);
    ASSERT_TRUE (q.pumpUntilIdle (kWaitMs));
    const auto after = *f.lib->tracks().findById (a);
    EXPECT_EQ (after.key, "Dm");
    EXPECT_FALSE (after.bpm.has_value());
    EXPECT_EQ (statusOf (f, a), PrepStatus::Done);
}

TEST (PrepQueue, NewResultsOverwriteOldAnalysis)
{
    Fixture f (1);
    const auto a = f.addTrack();
    f.lib->tracks().setAnalysis (a, 90.0, "C", "8B");
    PrepQueue q (*f.lib, *f.service, f.options (1));
    q.enqueue (a);
    ASSERT_TRUE (q.pumpUntilIdle (kWaitMs));
    EXPECT_DOUBLE_EQ (f.lib->tracks().findById (a)->bpm.value_or (0), 128.0);
    EXPECT_EQ (f.lib->tracks().findById (a)->camelot, "8A");
}

TEST (PrepQueue, TrackRemovedWhileQueuedDoesNotStallTheQueue)
{
    Fixture f (1);
    const auto a = f.addTrack(), b = f.addTrack();
    PrepQueue q (*f.lib, *f.service, f.options (1));
    q.enqueue (a);
    q.enqueue (b);
    f.lib->tracks().remove (a);   // cascades the queue row away
    ASSERT_TRUE (q.pumpUntilIdle (kWaitMs));
    EXPECT_EQ (statusOf (f, b), PrepStatus::Done);
}

TEST (PrepQueue, QueuePersistsAcrossRestartAndRunningResets)
{
    openmixer::library::test::TempDir tmp;
    const auto path = tmp.dir.getChildFile ("lib.db").getFullPathName();
    std::int64_t a = 0, b = 0;
    {
        Fixture f (1, path);
        a = f.addTrack();
        b = f.addTrack();
        f.lib->prepQueue().enqueue (a);
        f.lib->prepQueue().enqueue (b);
        f.lib->prepQueue().setStatus (a, PrepStatus::Running);   // as if the app died mid-job
    }
    Fixture f (1, path);
    PrepQueue q (*f.lib, *f.service, f.options (1));
    EXPECT_EQ (statusOf (f, a), PrepStatus::Queued);
    ASSERT_TRUE (q.pumpUntilIdle (kWaitMs));
    EXPECT_EQ (statusOf (f, a), PrepStatus::Done);
    EXPECT_EQ (statusOf (f, b), PrepStatus::Done);
}

TEST (PrepQueue, RemoveDropsRowAndCancelsRunningJob)
{
    Fixture f (1);
    auto gate = std::make_shared<GateAnalyzer>();
    f.replace (gate);
    const auto a = f.addTrack();
    PrepQueue q (*f.lib, *f.service, f.options (1));
    q.enqueue (a);
    q.pump();
    while (! gate->started.load())
        std::this_thread::sleep_for (std::chrono::milliseconds (1));
    ASSERT_TRUE (q.remove (a).wasOk());
    ASSERT_TRUE (q.pumpUntilIdle (kWaitMs));
    EXPECT_FALSE (f.lib->prepQueue().find (a).has_value());
}

TEST (PrepQueue, DestroyingWithJobsInFlightIsSafe)
{
    Fixture f (2);
    f.replace (std::make_shared<GateAnalyzer>());
    {
        PrepQueue q (*f.lib, *f.service, f.options (2));
        q.enqueue (f.addTrack());
        q.enqueue (f.addTrack());
        q.pump();
    }
    f.service->cancelAll();   // late listener callbacks must not touch the dead queue
    SUCCEED();
}
