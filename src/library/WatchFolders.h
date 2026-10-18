// Keeps watched folders in step with the library: the folder list lives in the DB, a background
// thread rescans every folder on an interval or on demand. The scan thread owns its own Library
// connection (WAL + busy timeout allow it next to the caller's); add/remove/list use a second one
// under a mutex, so the UI thread never waits on a scan.
// Needs a file-backed database (":memory:" is private to one connection).
#pragma once

#include "Ingestor.h"

#include <atomic>
#include <condition_variable>
#include <mutex>
#include <thread>

namespace openmixer::library
{

class WatchFolders
{
public:
    struct Options
    {
        std::int64_t intervalMs = 5 * 60 * 1000;                 // time between automatic rescans
        std::int64_t pollMs = 500;                               // how often the thread re-checks the clock
        std::function<std::int64_t()> clock;                     // ms; defaults to the system clock (inject in tests)
        std::function<void (const IngestSummary&)> onScanDone;   // called on the scan thread after each pass
        bool scanOnStart = true;
    };

    // nullptr (reason in `error`) when the database cannot be opened.
    static std::unique_ptr<WatchFolders> create (const juce::String& libraryPath, MetadataReaderRegistry readers,
                                                 Options options, juce::String* error = nullptr);
    ~WatchFolders();

    juce::Result add (const juce::File& folder);
    juce::Result remove (const juce::File& folder);
    std::vector<juce::String> list();

    void triggerScan();                  // request a pass now; returns immediately
    int scanCount() const noexcept { return passes.load(); }
    // Blocks until at least n passes finished (true) or the timeout hit (false).
    bool waitForScans (int n, int timeoutMs);
    // Wakes the thread to re-evaluate its clock (tests call this after advancing a fake clock).
    void poke();

    struct Key { explicit Key() = default; };
    WatchFolders (Key, std::unique_ptr<Library> control, juce::String path, MetadataReaderRegistry readers, Options options);

private:
    void run();
    bool due (std::int64_t& lastScan);
    IngestSummary scanAll (Library& scanLibrary);
    std::int64_t now() const;

    std::unique_ptr<Library> controlLibrary;   // guarded by controlMutex
    std::mutex controlMutex;
    juce::String libraryPath;
    MetadataReaderRegistry readers;
    Options options;

    std::mutex wakeMutex;
    std::condition_variable wake;
    std::condition_variable doneCv;
    bool triggered = false;
    bool stopping = false;
    std::atomic<int> passes { 0 };
    std::atomic<bool> stopFlag { false };
    std::thread worker;

    JUCE_DECLARE_NON_COPYABLE (WatchFolders)
};

} // namespace openmixer::library
