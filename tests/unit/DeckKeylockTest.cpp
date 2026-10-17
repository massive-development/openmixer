// OpenMixer DeckKeylockTest - key-lock keeps pitch while rate changes tempo; toggling stays continuous
#include "DeckTestUtil.h"

#include <chrono>
#include <cmath>
#include <vector>

using namespace deck_test;

namespace
{
constexpr double kToneHz = 440.0;
constexpr int kToneFrames = 66150;   // 1.5 s

juce::File writeTone (const juce::File& f)
{
    f.deleteFile();
    juce::WavAudioFormat wav;
    std::unique_ptr<juce::FileOutputStream> stream (f.createOutputStream());
    std::unique_ptr<juce::AudioFormatWriter> writer (wav.createWriterFor (stream.get(), kRate, 2, 24, {}, 0));
    stream.release();
    juce::AudioBuffer<float> buf (2, kToneFrames);
    for (int i = 0; i < kToneFrames; ++i)
        for (int c = 0; c < 2; ++c)
            buf.setSample (c, i, 0.5f * static_cast<float> (std::sin (2.0 * juce::MathConstants<double>::pi * kToneHz * i / kRate)));
    writer->writeFromAudioSampleBuffer (buf, 0, kToneFrames);
    return f;
}

std::vector<float> renderLeft (openmixer::Deck& deck, int numBlocks, const std::function<void (int)>& beforeBlock = {})
{
    std::vector<float> out;
    std::vector<float> l (256), r (256);
    for (int b = 0; b < numBlocks; ++b)
    {
        if (beforeBlock)
            beforeBlock (b);
        deck.renderStereo (l.data(), r.data(), 256);
        out.insert (out.end(), l.begin(), l.end());
        std::this_thread::sleep_for (std::chrono::milliseconds (1));
    }
    return out;
}

// Frequency from rising zero crossings over [from, to).
double estimateHz (const std::vector<float>& x, size_t from, size_t to)
{
    std::vector<double> crossings;
    for (size_t i = from + 1; i < to; ++i)
        if (x[i - 1] < 0.0f && x[i] >= 0.0f)
            crossings.push_back (static_cast<double> (i - 1) + x[i - 1] / (x[i - 1] - x[i]));
    if (crossings.size() < 2)
        return 0.0;
    return kRate * static_cast<double> (crossings.size() - 1) / (crossings.back() - crossings.front());
}

size_t longestSilentRun (const std::vector<float>& x, size_t from)
{
    size_t run = 0, best = 0;
    for (size_t i = from; i < x.size(); ++i)
    {
        run = std::abs (x[i]) < 1e-4f ? run + 1 : 0;
        best = std::max (best, run);
    }
    return best;
}

class KeylockFixture : public DeckFixture
{
protected:
    void SetUp() override
    {
        DeckFixture::SetUp();
        tone = writeTone (dir.getChildFile ("tone.wav"));
        deck.prepare (kRate, 256);
        ASSERT_TRUE (deck.load (tone));
        ASSERT_TRUE (pumpUntilBuffered (deck, 8192));
    }

    juce::File tone;
    openmixer::Deck deck;
};
using DeckKeylockSlow = KeylockFixture;
}

TEST_F (DeckKeylockSlow, KeylockHoldsPitchWhenRateChanges)
{
    deck.setKeylock (true);
    deck.setRate (1.25);
    deck.play();
    const auto out = renderLeft (deck, 120);
    EXPECT_NEAR (estimateHz (out, 8192, out.size()), kToneHz, kToneHz * 0.01);
    EXPECT_EQ (deck.getUnderrunCount(), 0U);
    // Tempo still follows rate: ~1.25 source frames per output frame.
    const double advance = static_cast<double> (deck.getPositionFrames()) / static_cast<double> (out.size());
    EXPECT_NEAR (advance, 1.25, 0.1);
}

TEST_F (DeckKeylockSlow, WithoutKeylockRateShiftsPitch)
{
    deck.setRate (1.25);
    deck.play();
    const auto out = renderLeft (deck, 60);
    EXPECT_NEAR (estimateHz (out, 1024, out.size()), kToneHz * 1.25, kToneHz * 0.01);
}

TEST_F (DeckKeylockSlow, TogglingKeylockWhilePlayingStaysAudibleAndFinite)
{
    deck.setRate (1.1);
    deck.play();
    const auto out = renderLeft (deck, 150, [&] (int block) { deck.setKeylock (block >= 40 && block < 100); });
    for (auto v : out)
        ASSERT_TRUE (std::isfinite (v));
    EXPECT_LT (longestSilentRun (out, 256), 64U) << "no audible dropout when switching paths";
    EXPECT_EQ (deck.getUnderrunCount(), 0U);
    EXPECT_TRUE (deck.setParameter ("keylock", 1.0));
    EXPECT_TRUE (deck.isKeylock());
}

TEST_F (DeckKeylockSlow, KeylockCompensatesADeviceRateMismatch)
{
    openmixer::Deck fast;
    fast.prepare (48000.0, 256);
    ASSERT_TRUE (fast.load (tone));
    ASSERT_TRUE (pumpUntilBuffered (fast, 8192));
    fast.setKeylock (true);
    fast.play();
    const auto out = renderLeft (fast, 120);
    EXPECT_NEAR (estimateHz (out, 8192, out.size()) * 48000.0 / kRate, kToneHz, kToneHz * 0.01);
}
