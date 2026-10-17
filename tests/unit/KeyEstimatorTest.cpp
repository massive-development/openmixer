// OpenMixer KeyEstimatorTest - key detection on synthetic chord progressions, Camelot mapping
#include <JuceHeader.h>
#include "analysis/KeyEstimator.h"
#include "AnalysisTestSignals.h"

#include <gtest/gtest.h>

#include <cmath>

using namespace openmixer::analysis;

namespace
{
constexpr double kChordSeconds = 1.5;

// Harmonic-rich note (4 partials, 1/n amplitudes) so the chroma has realistic overtones.
void addNote (AudioClip& clip, int midi, double from, double to)
{
    const auto hz = 440.0 * std::pow (2.0, (midi - 69) / 12.0);
    for (int partial = 1; partial <= 4; ++partial)
        test::addTone (clip, hz * partial, 0.08f / static_cast<float> (partial), from, to);
}

// I-IV-V-I (major) or i-iv-V-i (minor, raised leading tone) around the tonic, twice.
AudioClip cadence (int tonicMidi, bool minor)
{
    const auto third = minor ? 3 : 4;
    const std::vector<std::vector<int>> chords {
        { 0, third, 7 }, { 5, 5 + (minor ? 3 : 4), 12 }, { 7, 11, 14 }, { 0, third, 7 } };
    auto clip = test::silence (kChordSeconds * 8.0);
    for (int pass = 0; pass < 2; ++pass)
        for (size_t c = 0; c < chords.size(); ++c)
        {
            const auto from = kChordSeconds * static_cast<double> (pass * 4 + static_cast<int> (c));
            for (auto interval : chords[c])
                addNote (clip, tonicMidi + interval, from, from + kChordSeconds);
            addNote (clip, tonicMidi - 12, from, from + kChordSeconds);   // tonic pedal in the bass
        }
    return clip;
}

std::optional<KeyResult> estimate (const AudioClip& clip)
{
    NullAnalysisContext context;
    return KeyEstimator().estimate (clip, context);
}

struct KeyCase { int midi; bool minor; const char* name; const char* camelot; };

// gtest would otherwise dump raw bytes, including struct padding (Valgrind flags it).
void PrintTo (const KeyCase& c, std::ostream* os) { *os << c.name; }
class KeyAccuracy : public ::testing::TestWithParam<KeyCase> {};
} // namespace

TEST_P (KeyAccuracy, CadenceInKey)
{
    const auto& c = GetParam();
    const auto result = estimate (cadence (c.midi, c.minor));
    ASSERT_TRUE (result.has_value());
    EXPECT_EQ (result->key.getName(), c.name);
    EXPECT_EQ (result->key.getCamelot(), c.camelot);
    EXPECT_GT (result->confidence, 0.0);
}

INSTANTIATE_TEST_SUITE_P (Keys, KeyAccuracy, ::testing::Values (
    KeyCase { 60, false, "C", "8B" }, KeyCase { 57, true, "Am", "8A" }, KeyCase { 66, false, "F#", "2B" },
    KeyCase { 63, true, "D#m", "2A" }, KeyCase { 55, false, "G", "9B" }, KeyCase { 62, true, "Dm", "7A" },
    KeyCase { 58, false, "A#", "6B" }, KeyCase { 64, true, "Em", "9A" }));

TEST (KeyEstimator, SurvivesDrumsAndNoise)
{
    auto clip = cadence (62, false);   // D major
    const auto drums = test::clickTrack (124.0, clip.getDurationSeconds(), 0.0, 0);
    for (size_t i = 0; i < clip.samples.size(); ++i)
        clip.samples[i] += drums.samples[i];
    test::addNoise (clip, 0.02f);

    const auto result = estimate (clip);
    ASSERT_TRUE (result.has_value());
    EXPECT_EQ (result->key.getName(), "D");
}

TEST (KeyEstimator, SilenceHasNoKey)
{
    EXPECT_FALSE (estimate (test::silence (5.0)).has_value());
    KeyAnalyzer analyzer;
    NullAnalysisContext context;
    const auto parsed = KeyResult::fromJson (analyzer.analyze (test::silence (5.0), context).value_or (""));
    ASSERT_TRUE (parsed.has_value());
    EXPECT_FALSE (parsed->tonal);
}

TEST (KeyEstimator, ClassifiesChromaDirectly)
{
    Chroma cMajorScale { 1, 0, 1, 0, 1, 1, 0, 1, 0, 1, 0, 1 };
    cMajorScale[0] = 2.0;
    cMajorScale[7] = 1.5;
    const auto result = KeyEstimator::classify (cMajorScale);
    ASSERT_TRUE (result.has_value());
    EXPECT_EQ (result->key, (MusicalKey { 0, false }));
    EXPECT_DOUBLE_EQ (result->chroma[0], 1.0);
    EXPECT_FALSE (KeyEstimator::classify (Chroma {}).has_value());
}

TEST (KeyAnalyzer, PayloadRoundTrips)
{
    KeyAnalyzer analyzer;
    NullAnalysisContext context;
    const auto payload = analyzer.analyze (cadence (57, true), context);
    ASSERT_TRUE (payload.has_value());
    const auto parsed = KeyResult::fromJson (*payload);
    ASSERT_TRUE (parsed.has_value());
    EXPECT_TRUE (parsed->tonal);
    EXPECT_EQ (parsed->key, (MusicalKey { 9, true }));
    EXPECT_GT (parsed->chroma[9], 0.5);
    EXPECT_FALSE (KeyResult::fromJson ("{\"tonic\": 14}").has_value());
}

TEST (MusicalKey, CamelotWheelCoversAllKeys)
{
    juce::StringArray seen;
    for (int tonic = 0; tonic < 12; ++tonic)
        for (const bool minor : { false, true })
        {
            const MusicalKey key { tonic, minor };
            seen.addIfNotAlreadyThere (key.getCamelot());
            EXPECT_EQ (MusicalKey::fromCamelot (key.getCamelot()), key) << key.getName();
            EXPECT_EQ (MusicalKey::fromName (key.getName()), key) << key.getName();
            EXPECT_EQ (key.getRelative().getCamelotNumber(), key.getCamelotNumber());
        }
    EXPECT_EQ (seen.size(), 24);
    EXPECT_EQ ((MusicalKey { 5, false }).getCamelot(), "7B");    // F
    EXPECT_EQ ((MusicalKey { 11, true }).getCamelot(), "10A");   // Bm
    EXPECT_EQ (MusicalKey::fromName ("Bbm"), (MusicalKey { 10, true }));
    EXPECT_FALSE (MusicalKey::fromCamelot ("13A").has_value());
    EXPECT_FALSE (MusicalKey::fromCamelot ("8C").has_value());
    EXPECT_FALSE (MusicalKey::fromName ("H").has_value());
}

TEST (MusicalKey, HarmonicCompatibility)
{
    const MusicalKey c { 0, false }, g { 7, false }, f { 5, false }, am { 9, true }, d { 2, false }, em { 4, true };
    EXPECT_TRUE (isHarmonicallyCompatible (c, c));
    EXPECT_TRUE (isHarmonicallyCompatible (c, g));
    EXPECT_TRUE (isHarmonicallyCompatible (c, f));
    EXPECT_TRUE (isHarmonicallyCompatible (c, am));
    EXPECT_FALSE (isHarmonicallyCompatible (c, d));
    EXPECT_FALSE (isHarmonicallyCompatible (c, em));
}
