#include "LibraryServices.h"

#include "analysis/AnalysedBeatGrid.h"
#include "analysis/BpmEstimator.h"
#include "analysis/ClipSource.h"
#include "analysis/KeyEstimator.h"
#include "analysis/SqliteResultCache.h"
#include "analysis/StructureAnalyzer.h"
#include "analysis/WaveformBuilder.h"

namespace openmixer::ui
{

namespace
{
    constexpr const char* kMemoryPath = ":memory:";

    std::shared_ptr<analysis::AnalyzerRegistry> builtInAnalyzers()
    {
        auto registry = std::make_shared<analysis::AnalyzerRegistry>();
        registry->add (std::make_shared<analysis::BpmAnalyzer>());
        registry->add (std::make_shared<analysis::KeyAnalyzer>());
        registry->add (std::make_shared<analysis::WaveformAnalyzer>());
        registry->add (std::make_shared<analysis::StructureAnalyzer>());
        return registry;
    }

    // Cached payload for one analyser at its current version.
    std::optional<juce::String> cached (analysis::ResultCache* cache, const analysis::AnalyzerRegistry& analyzers,
                                        const juce::String& hash, const char* id)
    {
        const auto analyzer = analyzers.find (id);
        if (cache == nullptr || analyzer == nullptr || hash.isEmpty())
            return std::nullopt;
        return cache->find (hash, id, analyzer->getVersion());
    }
}

LibraryServices::Options LibraryServices::defaultOptions()
{
    Options o;
    const auto fromEnv = juce::SystemStats::getEnvironmentVariable ("OPENMIXER_LIBRARY_DB", {});
    o.dbPath = fromEnv.isNotEmpty() ? fromEnv
                                    : juce::File::getSpecialLocation (juce::File::userApplicationDataDirectory)
                                          .getChildFile ("OpenMixer").getChildFile ("library.db").getFullPathName();
    return o;
}

std::unique_ptr<LibraryServices> LibraryServices::create (Options options, juce::String* error)
{
    if (options.dbPath != kMemoryPath)
        juce::File (options.dbPath).getParentDirectory().createDirectory();
    auto library = library::Library::open (options.dbPath, library::Database::OpenMode::ReadWrite,
                                           library::coreMigrations(), error);
    if (library == nullptr)
        return nullptr;
    return std::make_unique<LibraryServices> (Key {}, std::move (library), std::move (options));
}

LibraryServices::LibraryServices (Key, std::unique_ptr<library::Library> library, Options options)
    : opts (std::move (options)), lib (std::move (library)), search (lib->database())
{
    startAnalysis();
    startWatching();
    startTimerHz (1000 / std::max (1, opts.pumpIntervalMs));
}

LibraryServices::~LibraryServices()
{
    *alive = false;   // queued error deliveries become no-ops
    stopTimer();   // then members unwind in reverse start-up order
}

void LibraryServices::report (const juce::String& message) const
{
    if (opts.onError)
        opts.onError (message);
    else
        juce::Logger::writeToLog ("library: " + message);
}

// A cache that cannot open is not fatal: tracks still load, they just never show cached waveforms.
void LibraryServices::startAnalysis()
{
    analyzers = opts.analyzers != nullptr ? opts.analyzers : builtInAnalyzers();
    const bool inMemory = opts.dbPath == kMemoryPath;
    const auto cachePath = opts.cachePath.isNotEmpty() ? opts.cachePath : inMemory ? juce::String (kMemoryPath) : opts.dbPath + ".analysis";
    juce::String cacheError;
    cache = analysis::SqliteResultCache::open (cachePath, &cacheError);
    if (cache == nullptr)
        report ("analysis cache unavailable: " + cacheError);
    service = std::make_unique<analysis::AnalysisService> (*analyzers, cache.get(), std::max (1, opts.analysisWorkers));

    library::PrepQueue::Options po;
    po.observer = [this] (const library::PrepEntry& e) { listeners.call ([&] (Listener& l) { l.prepEntryChanged (e); }); };
    po.onError = [this] (const juce::String& m) { report (m); };
    prep = std::make_unique<library::PrepQueue> (*lib, *service, std::move (po));
}

void LibraryServices::startWatching()
{
    if (! opts.watchFolders || opts.dbPath == kMemoryPath)
        return;
    library::WatchFolders::Options wo;
    wo.intervalMs = opts.scanIntervalMs;
    wo.scanOnStart = opts.scanOnStart;
    wo.onScanDone = [this] (const library::IngestSummary&) { ++finishedScans; };   // scan thread: only bump a counter
    wo.onError = [this, flag = alive] (const juce::String& m)   // scan thread: hop to the message thread
    {
        juce::MessageManager::callAsync ([this, flag, m] { if (*flag) report (m); });
    };
    juce::String error;
    watch = library::WatchFolders::create (opts.dbPath, library::MetadataReaderRegistry::withDefaults(), std::move (wo), &error);
    if (watch == nullptr)
        report ("watch folders unavailable: " + error);
}

void LibraryServices::notifyLibraryChanged()
{
    listeners.call ([] (Listener& l) { l.libraryChanged(); });
}

void LibraryServices::pump()
{
    prep->pump();
    if (const int scans = finishedScans.load(); scans != seenScans)
    {
        seenScans = scans;
        notifyLibraryChanged();
    }
}

juce::Result LibraryServices::importFile (const juce::File& xml, library::ImportReport& importReport)
{
    auto result = library::importLibraryFile (*lib, importers, xml, importReport);
    if (result.wasOk())
        notifyLibraryChanged();
    return result;
}

TrackAnalysis LibraryServices::analysisFor (const library::TrackRecord& track)
{
    const auto hash = track.contentHash.isNotEmpty() ? track.contentHash : analysis::hashFileContent (juce::File (track.path));
    TrackAnalysis out;
    const auto wave = cached (cache.get(), *analyzers, hash, "waveform");
    const auto bpm = cached (cache.get(), *analyzers, hash, "bpm");
    out.analysed = wave.has_value() && bpm.has_value();
    if (wave)
        if (auto data = analysis::WaveformData::fromJson (*wave); data && data->isValid())
            out.waveform = std::make_shared<const analysis::WaveformData> (std::move (*data));
    if (bpm)
        if (const auto result = analysis::BpmResult::fromJson (*bpm))
            out.grid = analysis::makeEngineGrid (result->grid);
    return out;
}

} // namespace openmixer::ui
