// Shared state of one submitted job. The service writes it from a worker thread; any thread may
// read it, cancel it or wait for it.
#pragma once

#include <juce_core/juce_core.h>

#include <atomic>
#include <condition_variable>
#include <map>
#include <mutex>

namespace openmixer::analysis
{

enum class JobStatus { Queued, Running, Done, Cancelled, Failed };

class AnalysisJob
{
public:
    AnalysisJob (juce::int64 jobId, juce::String sourceName) : id (jobId), name (std::move (sourceName)) {}

    juce::int64 getId() const noexcept { return id; }
    const juce::String& getSourceName() const noexcept { return name; }

    JobStatus getStatus() const;
    bool isFinished() const;
    float getProgress() const noexcept { return progress.load (std::memory_order_relaxed); }
    void cancel() noexcept { cancelled.store (true, std::memory_order_release); }
    bool isCancelled() const noexcept { return cancelled.load (std::memory_order_acquire); }

    // Analyser id -> JSON payload; complete once the status is Done.
    std::map<juce::String, juce::String> getResults() const;
    juce::StringArray getCachedIds() const;   // results that came from the cache
    juce::String getContentHash() const;
    juce::String getError() const;

    bool waitUntilFinished (int timeoutMs) const;

    // Service side.
    void setProgress (float value) noexcept { progress.store (value, std::memory_order_relaxed); }
    void setStatus (JobStatus newStatus);
    void setContentHash (const juce::String& hash);
    void addResult (const juce::String& analyzerId, const juce::String& payload, bool fromCache);
    void fail (const juce::String& message);

private:
    static bool isTerminal (JobStatus s) noexcept;

    const juce::int64 id;
    const juce::String name;
    std::atomic<float> progress { 0.0f };
    std::atomic<bool> cancelled { false };

    mutable std::mutex mutex;   // guards everything below
    mutable std::condition_variable finished;
    JobStatus status = JobStatus::Queued;
    std::map<juce::String, juce::String> results;
    juce::StringArray cachedIds;
    juce::String contentHash, error;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR (AnalysisJob)
};

} // namespace openmixer::analysis
