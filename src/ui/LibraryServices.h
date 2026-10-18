// Composition of the library subsystem for the app: the message-thread Library connection, search,
// analysis service + result cache, prep queue (pumped by a message-thread Timer) and watch folders
// (own scan thread and connection). Nothing here knows about widgets; the browser and track loader
// talk to it. Start-up order: Library (writer, migrates) -> result cache -> analysis service ->
// prep queue -> watch folders; shutdown is the reverse (members are declared in start-up order).
// Threading: every method runs on the message thread, which owns library() and prepQueue();
// never hand those to another thread. Preferences code adds/removes watch folders through
// watchFolders() and calls notifyLibraryChanged() so open views refresh.
#pragma once

#include "analysis/AnalysisService.h"
#include "engine/BeatGrid.h"
#include "library/ImportModel.h"
#include "library/LibraryImport.h"
#include "library/LibraryImporter.h"
#include "library/PrepQueue.h"
#include "library/SearchEngine.h"
#include "library/WatchFolders.h"

#include <juce_events/juce_events.h>

#include <atomic>
#include <memory>

namespace openmixer::analysis { class WaveformData; }

namespace openmixer::ui
{

// What the result cache already knows about a track; `analysed` once waveform and tempo are both cached.
struct TrackAnalysis
{
    std::shared_ptr<const analysis::WaveformData> waveform;
    std::shared_ptr<const BeatGrid> grid;   // null when no pulse was found
    bool analysed = false;
};

class LibraryServices : private juce::Timer
{
public:
    struct Options
    {
        juce::String dbPath;                    // ":memory:" runs without watch folders
        juce::String cachePath;                 // empty: "<dbPath>.analysis" (":memory:" for an in-memory library)
        bool watchFolders = true;
        bool scanOnStart = true;
        std::int64_t scanIntervalMs = 5 * 60 * 1000;
        int pumpIntervalMs = 100;               // prep queue / scan-done polling
        int analysisWorkers = 2;
        std::shared_ptr<analysis::AnalyzerRegistry> analyzers;   // null: bpm, key, waveform, structure
        std::function<void (const juce::String&)> onError;
    };

    // OPENMIXER_LIBRARY_DB overrides the default <user data>/OpenMixer/library.db.
    static Options defaultOptions();
    // nullptr (reason in `error`) when the library cannot be opened.
    static std::unique_ptr<LibraryServices> create (Options options, juce::String* error = nullptr);
    ~LibraryServices() override;

    class Listener
    {
    public:
        virtual ~Listener() = default;
        virtual void libraryChanged() {}                              // tracks or playlists changed
        virtual void prepEntryChanged (const library::PrepEntry&) {}  // prep queue status/progress
    };
    void addListener (Listener* l) { listeners.add (l); }
    void removeListener (Listener* l) { listeners.remove (l); }
    void notifyLibraryChanged();

    library::Library& getLibrary() noexcept { return *lib; }
    library::SearchEngine& getSearch() noexcept { return search; }
    library::PrepQueue& getPrepQueue() noexcept { return *prep; }
    library::WatchFolders* getWatchFolders() noexcept { return watch.get(); }   // null for an in-memory library
    library::LibraryImporterRegistry& getImporters() noexcept { return importers; }

    TrackAnalysis analysisFor (const library::TrackRecord& track);
    juce::Result importFile (const juce::File& xml, library::ImportReport& report);   // notifies on success
    void pump();   // one timer tick: prep queue + finished scans (tests call it directly)

    struct Key { explicit Key() = default; };
    LibraryServices (Key, std::unique_ptr<library::Library> library, Options options);

private:
    void timerCallback() override { pump(); }
    void startAnalysis();
    void startWatching();
    void report (const juce::String& message) const;

    Options opts;
    std::atomic<int> finishedScans { 0 };   // bumped on the scan thread, read by pump()
    int seenScans = 0;
    juce::ListenerList<Listener> listeners;
    std::unique_ptr<library::Library> lib;
    library::SearchEngine search;
    library::LibraryImporterRegistry importers = library::LibraryImporterRegistry::withDefaults();
    std::shared_ptr<analysis::AnalyzerRegistry> analyzers;
    std::unique_ptr<analysis::ResultCache> cache;
    std::unique_ptr<analysis::AnalysisService> service;
    std::unique_ptr<library::PrepQueue> prep;
    std::unique_ptr<library::WatchFolders> watch;
};

} // namespace openmixer::ui
