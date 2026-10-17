#include "AnalysisJob.h"

#include <chrono>

namespace openmixer::analysis
{

bool AnalysisJob::isTerminal (JobStatus s) noexcept
{
    return s == JobStatus::Done || s == JobStatus::Cancelled || s == JobStatus::Failed;
}

JobStatus AnalysisJob::getStatus() const
{
    const std::lock_guard<std::mutex> lock (mutex);
    return status;
}

bool AnalysisJob::isFinished() const
{
    return isTerminal (getStatus());
}

std::map<juce::String, juce::String> AnalysisJob::getResults() const
{
    const std::lock_guard<std::mutex> lock (mutex);
    return results;
}

juce::StringArray AnalysisJob::getCachedIds() const
{
    const std::lock_guard<std::mutex> lock (mutex);
    return cachedIds;
}

juce::String AnalysisJob::getContentHash() const
{
    const std::lock_guard<std::mutex> lock (mutex);
    return contentHash;
}

juce::String AnalysisJob::getError() const
{
    const std::lock_guard<std::mutex> lock (mutex);
    return error;
}

bool AnalysisJob::waitUntilFinished (int timeoutMs) const
{
    std::unique_lock<std::mutex> lock (mutex);
    return finished.wait_for (lock, std::chrono::milliseconds (timeoutMs), [this] { return isTerminal (status); });
}

void AnalysisJob::setStatus (JobStatus newStatus)
{
    {
        const std::lock_guard<std::mutex> lock (mutex);
        status = newStatus;
    }
    if (isTerminal (newStatus))
        finished.notify_all();
}

void AnalysisJob::setContentHash (const juce::String& hash)
{
    const std::lock_guard<std::mutex> lock (mutex);
    contentHash = hash;
}

void AnalysisJob::addResult (const juce::String& analyzerId, const juce::String& payload, bool fromCache)
{
    const std::lock_guard<std::mutex> lock (mutex);
    results[analyzerId] = payload;
    if (fromCache)
        cachedIds.add (analyzerId);
}

void AnalysisJob::fail (const juce::String& message)
{
    {
        const std::lock_guard<std::mutex> lock (mutex);
        error = message;
        status = JobStatus::Failed;
    }
    finished.notify_all();
}

} // namespace openmixer::analysis
