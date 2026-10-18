#include "WatchFolders.h"

namespace openmixer::library
{

std::unique_ptr<WatchFolders> WatchFolders::create (const juce::String& libraryPath, MetadataReaderRegistry readers,
                                                    Options options, juce::String* error)
{
    auto control = Library::open (libraryPath, Database::OpenMode::ReadWrite, options.migrations, error);
    if (control == nullptr)
        return nullptr;
    return std::make_unique<WatchFolders> (Key {}, std::move (control), libraryPath, std::move (readers), std::move (options));
}

WatchFolders::WatchFolders (Key, std::unique_ptr<Library> control, juce::String path, MetadataReaderRegistry r, Options o)
    : controlLibrary (std::move (control)), libraryPath (std::move (path)), readers (std::move (r)), options (std::move (o))
{
    triggered = options.scanOnStart;
    worker = std::thread ([this] { run(); });
}

WatchFolders::~WatchFolders()
{
    stopFlag = true;   // aborts a scan in progress between files
    {
        std::lock_guard<std::mutex> lock (wakeMutex);
        stopping = true;
    }
    wake.notify_all();
    if (worker.joinable())
        worker.join();
}

std::int64_t WatchFolders::now() const
{
    return options.clock ? options.clock() : static_cast<std::int64_t> (juce::Time::getMillisecondCounterHiRes());
}

juce::Result WatchFolders::add (const juce::File& folder)
{
    if (! folder.isDirectory())
        return juce::Result::fail ("not a folder: " + folder.getFullPathName());
    std::lock_guard<std::mutex> lock (controlMutex);
    auto r = controlLibrary->watchFolders().add (folder.getFullPathName());
    if (r.wasOk())
        triggerScan();
    return r;
}

juce::Result WatchFolders::remove (const juce::File& folder)
{
    std::lock_guard<std::mutex> lock (controlMutex);
    return controlLibrary->watchFolders().remove (folder.getFullPathName());
}

std::vector<juce::String> WatchFolders::list()
{
    std::lock_guard<std::mutex> lock (controlMutex);
    return controlLibrary->watchFolders().list();
}

void WatchFolders::triggerScan()
{
    {
        std::lock_guard<std::mutex> lock (wakeMutex);
        triggered = true;
    }
    wake.notify_all();
}

void WatchFolders::poke() { wake.notify_all(); }

bool WatchFolders::waitForScans (int n, int timeoutMs)
{
    std::unique_lock<std::mutex> lock (wakeMutex);
    return doneCv.wait_for (lock, std::chrono::milliseconds (timeoutMs), [&] { return passes.load() >= n; });
}

bool WatchFolders::due (std::int64_t& lastScan)
{
    const auto t = now();
    if (! triggered && t - lastScan < options.intervalMs)
        return false;
    triggered = false;
    lastScan = t;
    return true;
}

IngestSummary WatchFolders::scanAll (Library& scanLibrary)
{
    IngestSummary total;
    Ingestor ingestor (scanLibrary, readers);
    juce::Result listed = juce::Result::ok();
    const auto folders = scanLibrary.watchFolders().list (&listed);
    if (listed.failed())
        return total;   // an unreadable list is not an empty list; try again next pass
    for (const auto& path : folders)
    {
        IngestSummary one;
        ingestor.ingestFolder (juce::File (path), one, [this] { return stopFlag.load(); });   // missing folder: skipped
        total += one;
    }
    return total;
}

std::unique_ptr<Library> WatchFolders::openScanLibrary()
{
    juce::String error;
    auto lib = Library::open (libraryPath, Database::OpenMode::ReadWrite, options.migrations, &error);
    if (lib == nullptr && options.onError)
        options.onError ("watch folders: cannot open library: " + error);
    return lib;
}

void WatchFolders::run()
{
    // why: opened lazily on this thread so the connection is only ever used here; retried each pass if it failed
    std::unique_ptr<Library> scanLibrary;
    std::int64_t lastScan = now();

    std::unique_lock<std::mutex> lock (wakeMutex);
    while (! stopping)
    {
        if (! due (lastScan))
        {
            wake.wait_for (lock, std::chrono::milliseconds (options.pollMs));
            continue;
        }
        lock.unlock();
        if (scanLibrary == nullptr)
            scanLibrary = openScanLibrary();
        IngestSummary summary;
        if (scanLibrary != nullptr)
            summary = scanAll (*scanLibrary);
        else
            summary.failed = 1;   // surfaced to onScanDone so callers can tell the pass did nothing
        if (options.onScanDone)
            options.onScanDone (summary);
        lock.lock();
        ++passes;
        doneCv.notify_all();
    }
}

} // namespace openmixer::library
