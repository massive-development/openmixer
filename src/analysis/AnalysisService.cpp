#include "AnalysisService.h"

#include "JobRunner.h"

#include <algorithm>

namespace openmixer::analysis
{

AnalysisService::AnalysisService (const AnalyzerRegistry& analyzers, ResultCache* resultCache, int numWorkers)
    : registry (analyzers), cache (resultCache)
{
    for (int i = 0; i < std::max (1, numWorkers); ++i)
        workers.emplace_back ([this] { workerLoop(); });
}

AnalysisService::~AnalysisService()
{
    {
        const std::lock_guard<std::mutex> lock (mutex);
        stopping = true;
    }
    cancelAll();
    wake.notify_all();
    for (auto& w : workers)
        w.join();
}

std::shared_ptr<AnalysisJob> AnalysisService::submit (std::unique_ptr<ClipSource> source,
                                                      juce::StringArray analyzerIds, Listener onFinished)
{
    std::shared_ptr<AnalysisJob> job;
    {
        const std::lock_guard<std::mutex> lock (mutex);
        job = std::make_shared<AnalysisJob> (nextId++, source != nullptr ? source->describe() : juce::String());
        if (source == nullptr || stopping)
        {
            job->fail (source == nullptr ? "no source" : "service is shutting down");
            return job;
        }
        queue.push_back ({ job, std::move (source), std::move (analyzerIds), std::move (onFinished) });
    }
    wake.notify_one();
    return job;
}

void AnalysisService::cancelAll()
{
    const std::lock_guard<std::mutex> lock (mutex);
    for (auto& task : queue)
        task.job->cancel();
    for (auto& job : running)
        job->cancel();
}

size_t AnalysisService::getQueuedCount() const
{
    const std::lock_guard<std::mutex> lock (mutex);
    return queue.size();
}

std::optional<AnalysisService::Task> AnalysisService::waitForTask()
{
    std::unique_lock<std::mutex> lock (mutex);
    wake.wait (lock, [this] { return stopping || ! queue.empty(); });
    if (queue.empty())
        return std::nullopt;

    auto task = std::move (queue.front());
    queue.pop_front();
    running.push_back (task.job);
    return task;
}

void AnalysisService::workerLoop()
{
    while (auto task = waitForTask())
    {
        run (*task);
        const std::lock_guard<std::mutex> lock (mutex);
        running.erase (std::find (running.begin(), running.end(), task->job));
    }
}

void AnalysisService::run (Task& task) const
{
    runAnalysisJob (*task.job, *task.source, task.ids, registry, cache);
    if (task.listener)
        task.listener (*task.job);
}

} // namespace openmixer::analysis
