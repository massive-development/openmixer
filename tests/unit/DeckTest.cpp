// OpenMixer DeckTest - file-backed deck loading, playback, seek, rate and mock-device integration
#include <JuceHeader.h>
#include "engine/Deck.h"
#include "engine/MockAudioDevice.h"

#include <gtest/gtest.h>

#include <cmath>
#include <functional>
#include <thread>
#include <vector>

namespace
{
constexpr int kFrames = 20000;
constexpr double kRate = 44100.0;

/** Writes a WAV: left = n / kFrames, right = -left. */
juce::File writeRamp (const juce::File& f, int numChannels)
{
    f.deleteFile();
    juce::WavAudioFormat wav;
    std::unique_ptr<juce::FileOutputStream> stream (f.createOutputStream());
    std::unique_ptr<juce::AudioFormatWriter> writer (
        wav.createWriterFor (stream.get(), kRate, static_cast<unsigned> (numChannels), 24, {}, 0));
    stream.release();   // writer owns it now

    juce::AudioBuffer<float> buf (numChannels, kFrames);
    for (int i = 0; i < kFrames; ++i)
        for (int c = 0; c < numChannels; ++c)
            buf.setSample (c, i, (c == 0 ? 1.0f : -1.0f) * static_cast<float> (i) / static_cast<float> (kFrames));
    writer->writeFromAudioSampleBuffer (buf, 0, kFrames);
    return f;
}

/** Bounded poll: the deck's decoder thread exposes no signal to wait on. */
bool waitFor (const std::function<bool()>& cond, int timeoutMs = 3000)
{
    const auto end = juce::Time::getMillisecondCounter() + static_cast<juce::uint32> (timeoutMs);
    while (! cond())
    {
        if (juce::Time::getMillisecondCounter() > end) return false;
        std::this_thread::yield();
    }
    return true;
}

/** Pump blocks until the deck is at expectedPos with enough buffered, or timeout. */
bool pumpUntilBuffered (openmixer::Deck& deck, size_t frames, juce::int64 expectedPos = 0)
{
    float l[64], r[64];
    return waitFor ([&]
    {
        deck.renderStereo (l, r, 64);   // services the seek handshake while paused
        return deck.getPositionFrames() == expectedPos && deck.getBufferedFrames() >= frames;
    });
}

class DeckFixture : public ::testing::Test
{
protected:
    void SetUp() override
    {
        dir = juce::File::getSpecialLocation (juce::File::tempDirectory)
                  .getChildFile ("openmixer_deck_test_" + juce::Uuid().toString());   // unique per test process: ctest -j runs them in parallel
        ASSERT_TRUE (dir.createDirectory());
        wavFile = writeRamp (dir.getChildFile ("ramp.wav"), 2);
    }

    void TearDown() override { dir.deleteRecursively(); }

    juce::File dir, wavFile;
};
}

using DeckTest = DeckFixture;
using DeckPlaybackSlow = DeckFixture;

TEST_F (DeckTest, LoadRejectsNonAudioFiles)
{
    openmixer::Deck deck;
    auto junk = dir.getChildFile ("junk.wav");
    junk.replaceWithText ("not audio");
    EXPECT_FALSE (deck.load (junk));
    EXPECT_FALSE (deck.isLoaded());
}

TEST_F (DeckTest, LoadReportsLengthAndSampleRate)
{
    openmixer::Deck deck;
    ASSERT_TRUE (deck.load (wavFile));
    EXPECT_TRUE (deck.isLoaded());
    EXPECT_EQ (static_cast<int> (deck.getLengthFrames()), kFrames);
    EXPECT_EQ (deck.getFileSampleRate(), kRate);
}

TEST_F (DeckPlaybackSlow, PausedDeckOutputsSilenceAndDoesNotAdvance)
{
    openmixer::Deck deck;
    deck.prepare (kRate, 256);
    deck.load (wavFile);
    pumpUntilBuffered (deck, 1000);
    std::vector<float> l (256, 1.0f), r (256, 1.0f);
    deck.renderStereo (l.data(), r.data(), 256);
    for (auto v : l)
        EXPECT_EQ (v, 0.0f);
    EXPECT_EQ (static_cast<int> (deck.getPositionFrames()), 0);
}

TEST_F (DeckPlaybackSlow, PlaybackAtRateOneReproducesTheSourceAndFinishes)
{
    openmixer::Deck deck (1 << 12);   // small ring forces many wraparounds
    deck.prepare (kRate, 256);
    deck.load (wavFile);
    pumpUntilBuffered (deck, 512);
    deck.play();

    std::vector<float> out;
    std::vector<float> l (256), r (256);
    const bool ok = waitFor ([&]
    {
        // Pace like a device: only render when a full block is buffered
        // (or everything remaining is already buffered).
        if (deck.getBufferedFrames() < 300
            && deck.getPositionFrames() + static_cast<juce::int64> (deck.getBufferedFrames()) < kFrames - 3)
            return false;
        deck.renderStereo (l.data(), r.data(), 256);
        out.insert (out.end(), l.begin(), l.end());
        return deck.hasFinished();
    }, 5000);
    EXPECT_TRUE (ok) << "deck should reach the end";
    EXPECT_FALSE (deck.isPlaying());

    // Output frame i == source frame i until the final frame (the interpolation anchor, never emitted).
    ASSERT_GE (out.size(), static_cast<size_t> (kFrames) - 1);
    for (int i = 0; i < kFrames - 1; ++i)
    {
        const float expected = static_cast<float> (i) / static_cast<float> (kFrames);
        if (std::abs (out[static_cast<size_t> (i)] - expected) > 1e-4f)
        {
            ADD_FAILURE() << "mismatch at frame " << i;
            break;
        }
    }
}

TEST_F (DeckPlaybackSlow, UnderrunIsCountedAndOutputsSilenceWhenDecoderIsStarved)
{
    openmixer::Deck deck (1 << 12);
    deck.prepare (kRate, 256);
    deck.load (wavFile);
    pumpUntilBuffered (deck, 2048);
    deck.setRate (8.0);   // consume far faster than the ring can hold
    deck.play();
    std::vector<float> l (4096, 1.0f), r (4096, 1.0f);
    deck.renderStereo (l.data(), r.data(), 4096);
    EXPECT_GE (deck.getUnderrunCount(), 1) << "underrun should be counted";
    EXPECT_EQ (l[4095], 0.0f);   // tail is silence
    EXPECT_NE (l[1], 0.0f);      // head had real audio
    EXPECT_TRUE (deck.isPlaying()) << "an underrun is not end-of-file";
}

TEST_F (DeckPlaybackSlow, SeekRepositionsPlayback)
{
    openmixer::Deck deck;
    deck.prepare (kRate, 256);
    deck.load (wavFile);
    pumpUntilBuffered (deck, 512);
    deck.seek (10000);
    std::vector<float> l (64), r (64);
    const bool synced = waitFor ([&]
    {
        deck.renderStereo (l.data(), r.data(), 64);
        return deck.getPositionFrames() == 10000 && deck.getBufferedFrames() > 256;
    });
    ASSERT_TRUE (synced) << "seek handshake should complete";
    deck.play();
    waitFor ([&] { deck.renderStereo (l.data(), r.data(), 64); return l[0] != 0.0f; });
    EXPECT_NEAR (l[0], 10000.0f / static_cast<float> (kFrames), 1e-4f);
    EXPECT_NEAR (r[0], -10000.0f / static_cast<float> (kFrames), 1e-4f);
    EXPECT_NEAR (l[1] - l[0], 1.0f / static_cast<float> (kFrames), 1e-6f);
}

TEST_F (DeckPlaybackSlow, RateHalfAdvancesHalfAsFastAndInterpolates)
{
    openmixer::Deck deck;
    deck.prepare (kRate, 256);
    deck.load (wavFile);
    deck.seek (4000);
    deck.setRate (0.5);
    pumpUntilBuffered (deck, 2048, 4000);
    deck.play();
    std::vector<float> l (1000), r (1000);
    deck.renderStereo (l.data(), r.data(), 1000);
    EXPECT_LE (std::abs (deck.getPositionFrames() - (4000 + 500)), 2) << "position " << deck.getPositionFrames();
    // Linear ramp => interpolation is exact; slope is half.
    EXPECT_NEAR (l[2] - l[0], 1.0f / static_cast<float> (kFrames), 1e-6f);
}

TEST_F (DeckPlaybackSlow, MonoFilesPlayOnBothChannels)
{
    auto mono = writeRamp (dir.getChildFile ("mono.wav"), 1);
    openmixer::Deck deck;
    deck.prepare (kRate, 256);
    deck.load (mono);
    pumpUntilBuffered (deck, 512);
    deck.seek (5000);
    std::vector<float> l (64), r (64);
    waitFor ([&]
    {
        deck.renderStereo (l.data(), r.data(), 64);
        return deck.getPositionFrames() == 5000 && deck.getBufferedFrames() > 128;
    });
    deck.play();
    deck.renderStereo (l.data(), r.data(), 64);
    EXPECT_NEAR (l[0], r[0], 1e-7f);
    EXPECT_GT (l[0], 0.2f);
}

TEST_F (DeckPlaybackSlow, DeckRunsUnderAMockAudioDevice)
{
    openmixer::Deck deck;
    openmixer::MockAudioDevice dev (kRate, 256, 0, 2);
    deck.load (wavFile);
    dev.start (deck);
    dev.renderBlocks (1);
    waitFor ([&] { dev.renderBlock(); return deck.getBufferedFrames() > 1024; });
    deck.play();
    waitFor ([&] { dev.renderBlock(); return deck.getPositionFrames() > 1000; });
    EXPECT_GT (deck.getPositionFrames(), 1000);
    EXPECT_GT (dev.getLatencyStats().callbackCount, 0u);
    dev.stop();
}
