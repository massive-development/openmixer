#include "PrepQueue.h"

#include <algorithm>
#include <chrono>

namespace openmixer::library
{

PrepQueue::PrepQueue (Library& library, analysis::AnalysisService& svc, Options options)
    : lib (library), service (svc), opts (std::move (options))
{
    opts.maxInFlight = std::max (1, opts.maxInFlight);
    if (! opts.sourceFactory)
        opts.sourceFactory = [] (const TrackRecord& t) { return std::make_unique<analysis::FileClipSource> (juce::File (t.path)); };
    if (auto r = lib.prepQueue().resetRunning(); r.failed())
        error (r.getErrorMessage());
}

PrepQueue::~PrepQueue()
{
    for (auto& entry : inFlight)
        entry.second.job->cancel();
    for (auto& entry : draining)
        entry.job->cancel();
}

juce::Result PrepQueue::enqueue (std::int64_t trackId, bool urgent)
{
    if (auto r = lib.prepQueue().enqueue (trackId); r.failed())
        return r;
    if (urgent)
        if (auto r = lib.prepQueue().prioritise (trackId); r.failed())
            return r;
    notify (trackId);
    return juce::Result::ok();
}

juce::Result PrepQueue::prioritise (std::int64_t trackId)
{
    auto r = lib.prepQueue().prioritise (trackId);
    notify (trackId);
    return r;
}

juce::Result PrepQueue::move (std::int64_t trackId, int index)
{
    return lib.prepQueue().move (trackId, index);
}

juce::Result PrepQueue::remove (std::int64_t trackId)
{
    cancel (trackId);
    return lib.prepQueue().remove (trackId);
}

void PrepQueue::cancel (std::int64_t trackId)
{
    if (const auto it = inFlight.find (trackId); it != inFlight.end())
        return cancelRunning (it);

    juce::Result status = juce::Result::ok();
    const auto entry = lib.prepQueue().find (trackId, &status);
    if (status.failed())
        return error (status.getErrorMessage());
    if (entry && entry->status == PrepStatus::Queued)
    {
        if (auto r = lib.prepQueue().setStatus (trackId, PrepStatus::Cancelled); r.failed())
            error (r.getErrorMessage());
        notify (trackId);
    }
}

void PrepQueue::cancelRunning (std::map<std::int64_t, InFlight>::iterator it)
{
    const auto trackId = it->first;
    it->second.job->cancel();
    draining.push_back (std::move (it->second));   // so a re-enqueue can start a fresh job at once
    inFlight.erase (it);
    if (auto r = lib.prepQueue().setStatus (trackId, PrepStatus::Cancelled); r.failed())
        error (r.getErrorMessage());
    notify (trackId);
}

void PrepQueue::cancelAll()
{
    juce::Result status = juce::Result::ok();
    for (const auto& e : lib.prepQueue().all (&status))
        if (e.status == PrepStatus::Queued || e.status == PrepStatus::Running)
            cancel (e.trackId);
    if (status.failed())
        error (status.getErrorMessage());
}

juce::Result PrepQueue::clearFinished()
{
    return lib.prepQueue().clearFinished();
}

bool PrepQueue::pump()
{
    drain();
    fill();
    reportProgress();
    if (! inFlight.empty() || ! draining.empty())
        return true;
    juce::Result status = juce::Result::ok();
    const auto next = lib.prepQueue().nextQueued (&status);
    if (status.failed())
        error (status.getErrorMessage());
    return status.failed() || next.has_value();   // an unreadable queue is not known to be idle
}

bool PrepQueue::pumpUntilIdle (int timeoutMs)
{
    const auto deadline = std::chrono::steady_clock::now() + std::chrono::milliseconds (timeoutMs);
    while (pump())
    {
        const auto left = std::chrono::duration_cast<std::chrono::milliseconds> (deadline - std::chrono::steady_clock::now());
        if (left.count() <= 0)
            return false;
        waitForInbox (static_cast<int> (std::min<std::int64_t> (left.count(), 50)));
    }
    return true;
}

bool PrepQueue::waitForInbox (int timeoutMs)
{
    std::unique_lock<std::mutex> lock (inbox->mutex);
    return inbox->wake.wait_for (lock, std::chrono::milliseconds (timeoutMs), [this] { return ! inbox->done.empty(); });
}

std::vector<PrepEntry> PrepQueue::list (juce::Result* status)
{
    auto entries = lib.prepQueue().all (status);
    for (auto& e : entries)
        if (const auto it = inFlight.find (e.trackId); it != inFlight.end())
            e.progress = it->second.job->getProgress();
    return entries;
}

void PrepQueue::post (const std::shared_ptr<Inbox>& box, std::int64_t trackId, std::int64_t generation,
                     const analysis::AnalysisJob& job)
{
    Finished f { trackId, generation, job.getStatus(), job.getResults(), job.getError() };
    {
        std::lock_guard<std::mutex> lock (box->mutex);
        box->done.push_back (std::move (f));
    }
    box->wake.notify_all();
}

void PrepQueue::drain()
{
    std::vector<Finished> finished;
    {
        std::lock_guard<std::mutex> lock (inbox->mutex);
        finished.swap (inbox->done);
    }
    for (const auto& f : finished)
        complete (f);
}

void PrepQueue::complete (const Finished& finished)
{
    if (const auto d = std::find_if (draining.begin(), draining.end(),
                                     [&] (const InFlight& f) { return f.generation == finished.generation; });
        d != draining.end())
    {
        draining.erase (d);   // cancelled earlier: the row was already settled
        return;
    }
    const auto it = inFlight.find (finished.trackId);
    if (it == inFlight.end() || it->second.generation != finished.generation)
        return;
    inFlight.erase (it);
    switch (finished.status)
    {
        case analysis::JobStatus::Done:
            if (auto r = applyPrepResults (lib.tracks(), finished.trackId, finished.results); r.failed())
                return fail (finished.trackId, r.getErrorMessage());
            if (auto r = lib.prepQueue().setStatus (finished.trackId, PrepStatus::Done); r.failed())
                error (r.getErrorMessage());
            break;
        case analysis::JobStatus::Cancelled:
            if (auto r = lib.prepQueue().setStatus (finished.trackId, PrepStatus::Cancelled); r.failed())
                error (r.getErrorMessage());
            break;
        default:
            return fail (finished.trackId, finished.error);
    }
    notify (finished.trackId);
}

void PrepQueue::fail (std::int64_t trackId, const juce::String& message)
{
    if (auto r = lib.prepQueue().setStatus (trackId, PrepStatus::Failed, message); r.failed())
        error (r.getErrorMessage());
    notify (trackId);
}

void PrepQueue::fill()
{
    while (static_cast<int> (inFlight.size() + draining.size()) < opts.maxInFlight && submitNext())
    {
    }
}

bool PrepQueue::submitNext()
{
    juce::Result status = juce::Result::ok();
    const auto next = lib.prepQueue().nextQueued (&status);
    if (status.failed())
    {
        error (status.getErrorMessage());   // retry on the next pump
        return false;
    }
    if (! next)
        return false;

    const auto id = next->trackId;
    const auto track = lib.tracks().findById (id, &status);
    if (status.failed() || ! track)
        fail (id, status.failed() ? status.getErrorMessage() : juce::String ("track no longer exists"));
    else if (auto r = lib.prepQueue().setStatus (id, PrepStatus::Running); r.failed())
    {
        error (r.getErrorMessage());
        return false;
    }
    else
    {
        const auto box = inbox;
        const auto generation = nextGeneration++;
        auto& slot = inFlight[id];
        slot.generation = generation;
        slot.job = service.submit (opts.sourceFactory (*track), opts.analyzerIds, [box, id, generation] (const analysis::AnalysisJob& job)
        {
            post (box, id, generation, job);
        });
    }
    return true;
}

void PrepQueue::reportProgress()
{
    for (auto& entry : inFlight)
    {
        const auto progress = entry.second.job->getProgress();
        if (progress != entry.second.reported)
        {
            entry.second.reported = progress;
            notify (entry.first);
        }
    }
}

void PrepQueue::notify (std::int64_t trackId)
{
    if (! opts.observer)
        return;
    juce::Result status = juce::Result::ok();
    auto entry = lib.prepQueue().find (trackId, &status);
    if (status.failed())
        error (status.getErrorMessage());
    if (entry)
    {
        if (const auto it = inFlight.find (trackId); it != inFlight.end())
            entry->progress = it->second.job->getProgress();
        opts.observer (*entry);
    }
}

void PrepQueue::error (const juce::String& message) const
{
    if (opts.onError)
        opts.onError (message);
}

} // namespace openmixer::library
