// Track prep queue: feeds the persisted queue (PrepQueueDao) to the shared AnalysisService and
// writes bpm/key results back into the library.
//
// Threading: owned by the thread that owns the Library connection (the "owner thread"). Every
// public method, every Library access and every Observer call happens on that thread; analysis
// worker threads only push finished jobs into a mutex-guarded inbox, so a sqlite connection is
// never shared. The owner calls pump() regularly (a UI timer, or pumpUntilIdle() in tests):
// pump() applies finished results, then tops up the service. Only maxInFlight jobs are ever
// submitted at once, so the order of the rest stays editable (move, prioritise, cancel).
// Library depends on analysis (same tier) for AnalysisService; it does not touch the app.
#pragma once

#include "Library.h"
#include "PrepResults.h"

#include "analysis/AnalysisService.h"

#include <condition_variable>
#include <functional>
#include <map>
#include <mutex>

namespace openmixer::library
{

class PrepQueue
{
public:
    using SourceFactory = std::function<std::unique_ptr<analysis::ClipSource> (const TrackRecord&)>;
    // Owner thread, from pump() or the mutating call: an item changed status or progress.
    using Observer = std::function<void (const PrepEntry&)>;

    struct Options
    {
        juce::StringArray analyzerIds { "bpm", "key", "waveform", "structure" };
        int maxInFlight = 2;                         // jobs handed to the service at once
        SourceFactory sourceFactory;                 // default: FileClipSource on the track path
        Observer observer;
        std::function<void (const juce::String&)> onError;   // database or write-back failures
    };

    // Library and service must outlive the queue. Entries left Running by a crash go back to Queued.
    PrepQueue (Library& library, analysis::AnalysisService& service, Options options);
    ~PrepQueue();   // cancels in-flight jobs; their rows stay Running until the next start resets them

    juce::Result enqueue (std::int64_t trackId, bool urgent = false);   // urgent: runs before everything queued
    juce::Result prioritise (std::int64_t trackId);
    juce::Result move (std::int64_t trackId, int index);
    juce::Result remove (std::int64_t trackId);   // cancels it first when running
    void cancel (std::int64_t trackId);           // Queued or Running -> Cancelled
    void cancelAll();
    juce::Result clearFinished();

    // Applies finished jobs and submits the next ones. Returns true while work remains.
    bool pump();
    // Pumps until the queue is idle (true) or the timeout hits (false); sleeps on the inbox between rounds.
    bool pumpUntilIdle (int timeoutMs);

    std::vector<PrepEntry> list (juce::Result* status = nullptr);   // user order, live progress for running items

private:
    struct Finished   // snapshot taken on the worker thread; the owner thread applies it
    {
        std::int64_t trackId = 0;
        std::int64_t generation = 0;   // which submit this result belongs to
        analysis::JobStatus status = analysis::JobStatus::Done;
        std::map<juce::String, juce::String> results;
        juce::String error;
    };
    struct Inbox
    {
        std::mutex mutex;   // guards done
        std::condition_variable wake;
        std::vector<Finished> done;
    };
    struct InFlight { std::shared_ptr<analysis::AnalysisJob> job; std::int64_t generation = 0; float reported = 0.0f; };
    static void post (const std::shared_ptr<Inbox>& box, std::int64_t trackId, std::int64_t generation,
                      const analysis::AnalysisJob& job);

    void drain();
    void complete (const Finished& finished);
    void cancelRunning (std::map<std::int64_t, InFlight>::iterator it);
    void fill();
    bool submitNext();
    void reportProgress();
    void notify (std::int64_t trackId);
    void fail (std::int64_t trackId, const juce::String& message);
    void error (const juce::String& message) const;
    bool waitForInbox (int timeoutMs);

    Library& lib;
    analysis::AnalysisService& service;
    Options opts;
    std::shared_ptr<Inbox> inbox = std::make_shared<Inbox>();   // shared with listeners that may outlive us
    std::map<std::int64_t, InFlight> inFlight;                  // owner thread only
    std::vector<InFlight> draining;   // cancelled jobs still unwinding; their results are ignored
    std::int64_t nextGeneration = 1;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR (PrepQueue)
};

} // namespace openmixer::library
