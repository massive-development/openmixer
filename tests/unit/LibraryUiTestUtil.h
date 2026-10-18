// Shared fixtures for the library UI tests: services over an in-memory library with instant fake
// analysers, and helpers that seed tracks.
#pragma once

#include <JuceHeader.h>
#include "LibraryIngestTestUtil.h"
#include "analysis/Analyzer.h"
#include "analysis/BpmEstimator.h"
#include "analysis/KeyEstimator.h"
#include "analysis/WaveformData.h"
#include "ui/LibraryServices.h"

#include <gtest/gtest.h>

#include <chrono>
#include <thread>

namespace libui
{

struct FixedAnalyzer final : openmixer::analysis::Analyzer
{
    FixedAnalyzer (juce::String analyzerId, juce::String result) : name (std::move (analyzerId)), payload (std::move (result)) {}
    juce::String getId() const override { return name; }
    int getVersion() const override { return 1; }
    std::optional<juce::String> analyze (const openmixer::analysis::AudioClip&, openmixer::analysis::AnalysisContext&) const override
    {
        return payload;
    }
    juce::String name, payload;
};

inline juce::String waveformJson()
{
    openmixer::analysis::WaveformLevel base;
    base.samplesPerPoint = 64;
    for (int i = 0; i < 64; ++i)
        base.points.push_back ({ -0.5F, 0.5F, 0.4F, 0.3F, 0.2F });
    return openmixer::analysis::WaveformData (8000.0, 64 * 64, std::move (base)).toJson();
}

inline juce::String bpmJson (double bpm)
{
    openmixer::analysis::BpmResult r;
    r.bpm = bpm;
    r.confidence = 0.9;
    r.grid = openmixer::analysis::BeatGridModel::constant (bpm, 8000.0, 0.0);
    return r.toJson();
}

inline std::shared_ptr<openmixer::analysis::AnalyzerRegistry> fakeAnalyzers()
{
    using namespace openmixer::analysis;
    auto registry = std::make_shared<AnalyzerRegistry>();
    registry->add (std::make_shared<FixedAnalyzer> ("bpm", bpmJson (128.0)));
    KeyResult key;
    key.key = { 9, true };   // A minor = 8A
    registry->add (std::make_shared<FixedAnalyzer> ("key", key.toJson()));
    registry->add (std::make_shared<FixedAnalyzer> ("waveform", waveformJson()));
    registry->add (std::make_shared<FixedAnalyzer> ("structure", "{}"));
    return registry;
}

inline openmixer::ui::LibraryServices::Options memoryOptions()
{
    openmixer::ui::LibraryServices::Options o;
    o.dbPath = ":memory:";
    o.analyzers = fakeAnalyzers();
    o.onError = [] (const juce::String& m) { ADD_FAILURE() << m; };
    return o;
}

inline std::unique_ptr<openmixer::ui::LibraryServices> makeServices (openmixer::ui::LibraryServices::Options o = memoryOptions())
{
    juce::String error;
    auto services = openmixer::ui::LibraryServices::create (std::move (o), &error);
    EXPECT_NE (services, nullptr) << error;
    return services;
}

struct Seed
{
    const char* title;
    const char* artist;
    std::optional<double> bpm;
    const char* camelot;
    const char* genre;
    int rating;
};

inline std::int64_t addTrack (openmixer::library::Library& lib, const Seed& s, const juce::String& path = {})
{
    static int counter = 0;
    openmixer::library::TrackRecord t;
    t.path = path.isNotEmpty() ? path : "/music/" + juce::String (++counter) + "_" + s.title + ".wav";
    t.title = s.title;
    t.artist = s.artist;
    t.bpm = s.bpm;
    t.camelot = s.camelot;
    t.genre = s.genre;
    t.rating = s.rating;
    t.durationSec = 245.0;
    return lib.tracks().insert (t).value_or (0);
}

inline std::vector<std::int64_t> seedFour (openmixer::library::Library& lib)
{
    return { addTrack (lib, { "Alpha", "Zed", 120.0, "8A", "House", 5 }),
             addTrack (lib, { "Bravo", "Yan", 128.0, "9A", "Techno", 3 }),
             addTrack (lib, { "Charlie", "Xi", 124.0, "3B", "House", 1 }),
             addTrack (lib, { "Delta", "Wu", std::nullopt, "", "", 0 }) };
}

// Pumps the services until `done` holds or the timeout passes.
template <typename Done>
bool pumpUntil (openmixer::ui::LibraryServices& services, Done done, int timeoutMs = 8000)
{
    const auto deadline = std::chrono::steady_clock::now() + std::chrono::milliseconds (timeoutMs);
    while (std::chrono::steady_clock::now() < deadline)
    {
        services.pump();
        if (done())
            return true;
        std::this_thread::sleep_for (std::chrono::milliseconds (5));
    }
    return false;
}

} // namespace libui
