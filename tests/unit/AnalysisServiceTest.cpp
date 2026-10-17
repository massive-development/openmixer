// OpenMixer AnalysisServiceTest - background jobs, cache hits, progress, cancellation (real threads)
#include <JuceHeader.h>
#include "analysis/AnalysisService.h"
#include "analysis/SqliteResultCache.h"
#include "AnalysisTestSignals.h"

#include <gtest/gtest.h>

#include <atomic>
#include <thread>

using namespace openmixer::analysis;

namespace
{
constexpr int kWaitMs = 10000;

// Reports the clip length and counts how often it actually ran.
struct CountingAnalyzer final : Analyzer
{
    explicit CountingAnalyzer (juce::String analyzerId) : name (std::move (analyzerId)) {}
    juce::String getId() const override { return name; }
    int getVersion() const override { return 1; }
    std::optional<juce::String> analyze (const AudioClip& clip, AnalysisContext& context) const override
    {
        calls.fetch_add (1);
        context.setProgress (0.5f);
        return juce::String (static_cast<int> (clip.samples.size()));
    }
    juce::String name;
    mutable std::atomic<int> calls { 0 };
};

// Spins until cancelled, so tests control exactly when it ends.
struct BlockingAnalyzer final : Analyzer
{
    juce::String getId() const override { return "block"; }
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

struct FailingAnalyzer final : Analyzer
{
    juce::String getId() const override { return "fail"; }
    int getVersion() const override { return 1; }
    std::optional<juce::String> analyze (const AudioClip&, AnalysisContext&) const override { return std::nullopt; }
};

std::unique_ptr<ClipSource> clipSource (double seconds = 0.1)
{
    return std::make_unique<MemoryClipSource> (test::silence (seconds), "silence");
}

struct Fixture
{
    Fixture()
    {
        registry.add (counting);
        registry.add (other);
        registry.add (blocking);
        registry.add (std::make_shared<FailingAnalyzer>());
    }

    AnalyzerRegistry registry;
    std::shared_ptr<CountingAnalyzer> counting = std::make_shared<CountingAnalyzer> ("count");
    std::shared_ptr<CountingAnalyzer> other = std::make_shared<CountingAnalyzer> ("other");
    std::shared_ptr<BlockingAnalyzer> blocking = std::make_shared<BlockingAnalyzer>();
    std::unique_ptr<SqliteResultCache> cache = SqliteResultCache::open (":memory:");
};
} // namespace

TEST (AnalysisServiceSlow, RunsAnalysersAndReportsResults)
{
    Fixture f;
    std::atomic<int> listenerCalls { 0 };
    AnalysisService service (f.registry, f.cache.get(), 2);

    auto job = service.submit (clipSource(), { "count", "other" }, [&] (const AnalysisJob&) { listenerCalls++; });
    ASSERT_TRUE (job->waitUntilFinished (kWaitMs));

    EXPECT_EQ (job->getStatus(), JobStatus::Done);
    EXPECT_FLOAT_EQ (job->getProgress(), 1.0f);
    const auto results = job->getResults();
    EXPECT_EQ (results.at ("count"), "4410");
    EXPECT_EQ (results.at ("other"), "4410");
    EXPECT_EQ (job->getContentHash().length(), 64);
    service.cancelAll();   // no-op once done
    for (int i = 0; i < 1000 && listenerCalls.load() == 0; ++i)
        std::this_thread::sleep_for (std::chrono::milliseconds (1));
    EXPECT_EQ (listenerCalls.load(), 1);
}

TEST (AnalysisServiceSlow, SecondJobForSameContentIsServedFromCache)
{
    Fixture f;
    AnalysisService service (f.registry, f.cache.get());

    auto first = service.submit (clipSource(), { "count" });
    ASSERT_TRUE (first->waitUntilFinished (kWaitMs));
    auto second = service.submit (clipSource(), { "count", "other" });
    ASSERT_TRUE (second->waitUntilFinished (kWaitMs));

    EXPECT_EQ (f.counting->calls.load(), 1);
    EXPECT_EQ (f.other->calls.load(), 1);
    EXPECT_EQ (second->getCachedIds(), juce::StringArray ("count"));
    EXPECT_EQ (second->getResults().at ("count"), "4410");
}

TEST (AnalysisServiceSlow, WorksWithoutCache)
{
    Fixture f;
    AnalysisService service (f.registry, nullptr);
    auto job = service.submit (clipSource(), { "count" });
    ASSERT_TRUE (job->waitUntilFinished (kWaitMs));
    EXPECT_EQ (job->getStatus(), JobStatus::Done);
}

TEST (AnalysisServiceSlow, CancelStopsRunningAndQueuedJobs)
{
    Fixture f;
    AnalysisService service (f.registry, f.cache.get(), 1);

    auto running = service.submit (clipSource(), { "block" });
    auto queued = service.submit (clipSource (0.2), { "count" });
    for (int i = 0; i < kWaitMs && ! f.blocking->started.load(); ++i)
        std::this_thread::sleep_for (std::chrono::milliseconds (1));
    EXPECT_EQ (service.getQueuedCount(), 1u);

    queued->cancel();
    running->cancel();
    ASSERT_TRUE (running->waitUntilFinished (kWaitMs));
    ASSERT_TRUE (queued->waitUntilFinished (kWaitMs));
    EXPECT_EQ (running->getStatus(), JobStatus::Cancelled);
    EXPECT_EQ (queued->getStatus(), JobStatus::Cancelled);
    EXPECT_EQ (f.counting->calls.load(), 0);
    EXPECT_FALSE (f.cache->find (running->getContentHash(), "block", 1).has_value());
}

TEST (AnalysisServiceSlow, FailuresAreReported)
{
    Fixture f;
    AnalysisService service (f.registry, f.cache.get());

    auto unknown = service.submit (clipSource(), { "nope" });
    auto failing = service.submit (clipSource(), { "fail" });
    auto unreadable = service.submit (std::make_unique<FileClipSource> (juce::File ("/nonexistent.wav")), { "count" });
    auto null = service.submit (nullptr, { "count" });

    for (const auto& job : { unknown, failing, unreadable, null })
    {
        ASSERT_TRUE (job->waitUntilFinished (kWaitMs));
        EXPECT_EQ (job->getStatus(), JobStatus::Failed);
        EXPECT_TRUE (job->getError().isNotEmpty());
    }
}

TEST (AnalysisServiceSlow, DestructorCancelsOutstandingJobs)
{
    Fixture f;
    std::shared_ptr<AnalysisJob> running, queued;
    {
        AnalysisService service (f.registry, f.cache.get(), 1);
        running = service.submit (clipSource(), { "block" });
        queued = service.submit (clipSource(), { "count" });
        for (int i = 0; i < kWaitMs && ! f.blocking->started.load(); ++i)
            std::this_thread::sleep_for (std::chrono::milliseconds (1));
    }
    EXPECT_EQ (running->getStatus(), JobStatus::Cancelled);
    EXPECT_EQ (queued->getStatus(), JobStatus::Cancelled);
}

TEST (AnalysisServiceSlow, AnalysesWavFilesWithProgress)
{
    Fixture f;
    const juce::TemporaryFile temp (".wav");
    juce::AudioBuffer<float> buffer (1, 44100);
    buffer.clear();
    test::writeWav (temp.getFile(), buffer, 44100.0);

    AnalysisService service (f.registry, f.cache.get());
    auto job = service.submit (std::make_unique<FileClipSource> (temp.getFile()), { "count" });
    ASSERT_TRUE (job->waitUntilFinished (kWaitMs));
    EXPECT_EQ (job->getResults().at ("count"), "44100");
    EXPECT_EQ (job->getContentHash(), hashFileContent (temp.getFile()));
}
