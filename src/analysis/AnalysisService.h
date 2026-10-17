// Background analysis: a FIFO of jobs served by worker threads, with per-job progress and
// cancellation and a shared result cache. submit()/cancelAll() may be called from any thread.
// Listeners run on a worker thread after the job reaches a final status; keep them short and
// hop to the message thread for UI work. Registry and cache must outlive the service.
#pragma once

#include "AnalysisJob.h"
#include "AnalyzerRegistry.h"
#include "ClipSource.h"
#include "ResultCache.h"

#include <deque>
#include <functional>
#include <thread>
#include <vector>

namespace openmixer::analysis
{

class AnalysisService
{
public:
    using Listener = std::function<void (const AnalysisJob&)>;

    AnalysisService (const AnalyzerRegistry& analyzers, ResultCache* resultCache, int numWorkers = 1);
    ~AnalysisService();   // cancels queued and running jobs, then joins the workers

    std::shared_ptr<AnalysisJob> submit (std::unique_ptr<ClipSource> source, juce::StringArray analyzerIds,
                                         Listener onFinished = {});
    void cancelAll();
    size_t getQueuedCount() const;

private:
    struct Task
    {
        std::shared_ptr<AnalysisJob> job;
        std::unique_ptr<ClipSource> source;
        juce::StringArray ids;
        Listener listener;
    };

    void workerLoop();
    std::optional<Task> waitForTask();
    void run (Task& task) const;

    const AnalyzerRegistry& registry;
    ResultCache* const cache;

    mutable std::mutex mutex;   // guards queue, running, stopping, nextId
    std::condition_variable wake;
    std::deque<Task> queue;
    std::vector<std::shared_ptr<AnalysisJob>> running;
    bool stopping = false;
    juce::int64 nextId = 1;

    std::vector<std::thread> workers;   // last: started after the state above exists

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR (AnalysisService)
};

} // namespace openmixer::analysis
