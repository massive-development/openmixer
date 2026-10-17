// OpenMixer AnalysisCacheTest - SQLite result cache, analyser registry and content hashing
#include <JuceHeader.h>
#include "analysis/AnalyzerRegistry.h"
#include "analysis/ClipSource.h"
#include "analysis/SqliteResultCache.h"
#include "AnalysisTestSignals.h"

#include <gtest/gtest.h>

using namespace openmixer::analysis;

namespace
{
struct NamedAnalyzer final : Analyzer
{
    explicit NamedAnalyzer (juce::String analyzerId, int iface = kInterfaceVersion) : name (std::move (analyzerId)), interfaceVersion (iface) {}
    int getInterfaceVersion() const noexcept override { return interfaceVersion; }
    juce::String getId() const override { return name; }
    int getVersion() const override { return 1; }
    std::optional<juce::String> analyze (const AudioClip&, AnalysisContext&) const override { return juce::String ("{}"); }
    juce::String name;
    int interfaceVersion;
};

std::unique_ptr<SqliteResultCache> memoryCache()
{
    juce::String error;
    auto cache = SqliteResultCache::open (":memory:", &error);
    EXPECT_NE (cache, nullptr) << error;
    return cache;
}

AudioClip ramp (int n, double rate)
{
    AudioClip clip;
    clip.sampleRate = rate;
    for (int i = 0; i < n; ++i)
        clip.samples.push_back (static_cast<float> (i) / static_cast<float> (n));
    return clip;
}
} // namespace

TEST (AnalysisCache, StoresAndFindsByHashAnalyzerAndVersion)
{
    auto cache = memoryCache();
    ASSERT_TRUE (cache->store ("abc", "bpm", 2, "{\"bpm\":128}"));

    EXPECT_EQ (cache->find ("abc", "bpm", 2).value_or (""), "{\"bpm\":128}");
    EXPECT_FALSE (cache->find ("abc", "bpm", 1).has_value());
    EXPECT_FALSE (cache->find ("abc", "key", 2).has_value());
    EXPECT_FALSE (cache->find ("xyz", "bpm", 2).has_value());
}

TEST (AnalysisCache, StoreReplacesAndEraseRemovesAllRowsForHash)
{
    auto cache = memoryCache();
    cache->store ("abc", "bpm", 1, "old");
    cache->store ("abc", "bpm", 2, "new");
    cache->store ("abc", "key", 1, "8A");
    cache->store ("def", "bpm", 1, "other");

    EXPECT_EQ (cache->find ("abc", "bpm", 2).value_or (""), "new");
    EXPECT_EQ (cache->erase ("abc"), 2);
    EXPECT_FALSE (cache->find ("abc", "key", 1).has_value());
    EXPECT_TRUE (cache->find ("def", "bpm", 1).has_value());
}

TEST (AnalysisCache, PersistsAcrossReopen)
{
    const juce::TemporaryFile temp (".sqlite");
    const auto path = temp.getFile().getFullPathName();
    {
        auto cache = SqliteResultCache::open (path);
        ASSERT_NE (cache, nullptr);
        cache->store ("h", "waveform", 3, juce::String::fromUTF8 ("unicode \xc3\xa9"));
    }
    auto reopened = SqliteResultCache::open (path);
    ASSERT_NE (reopened, nullptr);
    EXPECT_EQ (reopened->find ("h", "waveform", 3).value_or (""), juce::String::fromUTF8 ("unicode \xc3\xa9"));
}

TEST (AnalysisCache, OpenFailsForUnwritablePath)
{
    juce::String error;
    EXPECT_EQ (SqliteResultCache::open ("/nonexistent-dir/x/y.sqlite", &error), nullptr);
    EXPECT_TRUE (error.isNotEmpty());
}

TEST (AnalyzerRegistry, RejectsDuplicatesEmptyIdsAndWrongInterface)
{
    AnalyzerRegistry registry;
    EXPECT_TRUE (registry.add (std::make_shared<NamedAnalyzer> ("bpm")).wasOk());
    EXPECT_TRUE (registry.add (std::make_shared<NamedAnalyzer> ("bpm")).failed());
    EXPECT_TRUE (registry.add (std::make_shared<NamedAnalyzer> ("")).failed());
    EXPECT_TRUE (registry.add (std::make_shared<NamedAnalyzer> ("v0", 0)).failed());
    EXPECT_TRUE (registry.add (nullptr).failed());

    EXPECT_EQ (registry.getIds(), juce::StringArray ("bpm"));
    EXPECT_NE (registry.find ("bpm"), nullptr);
    EXPECT_TRUE (registry.remove ("bpm"));
    EXPECT_EQ (registry.find ("bpm"), nullptr);
}

TEST (ContentHash, ClipHashDependsOnSamplesAndRate)
{
    const auto a = hashClip (ramp (100, 44100.0));
    EXPECT_EQ (a.length(), 64);
    EXPECT_EQ (a, hashClip (ramp (100, 44100.0)));
    EXPECT_NE (a, hashClip (ramp (101, 44100.0)));
    EXPECT_NE (a, hashClip (ramp (100, 48000.0)));
}

TEST (ContentHash, FileSourceDecodesMonoAndHashesBytesNotName)
{
    const juce::TemporaryFile tempA (".wav"), tempB (".wav");
    juce::AudioBuffer<float> stereo (2, 1000);
    for (int i = 0; i < 1000; ++i)
    {
        stereo.setSample (0, i, 0.5f);
        stereo.setSample (1, i, -0.25f);
    }
    test::writeWav (tempA.getFile(), stereo, 48000.0);
    test::writeWav (tempB.getFile(), stereo, 48000.0);

    FileClipSource a (tempA.getFile()), b (tempB.getFile());
    EXPECT_EQ (a.getContentHash(), b.getContentHash());

    NullAnalysisContext context;
    const auto clip = a.load (context);
    ASSERT_TRUE (clip.has_value());
    EXPECT_EQ (clip->sampleRate, 48000.0);
    ASSERT_EQ (clip->samples.size(), 1000u);
    EXPECT_NEAR (clip->samples[500], 0.125f, 1.0e-5f);

    FileClipSource missing (juce::File ("/nonexistent/file.wav"));
    EXPECT_TRUE (missing.getContentHash().isEmpty());
    EXPECT_FALSE (missing.load (context).has_value());
}
