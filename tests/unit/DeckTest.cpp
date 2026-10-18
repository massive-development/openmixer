// OpenMixer DeckTest - file-backed deck loading, playback, seek, rate and mock-device integration
#include <JuceHeader.h>
#include "engine/Deck.h"
#include "engine/MockAudioDevice.h"
#include "DeckTestUtil.h"

#include <gtest/gtest.h>

#include <cmath>
#include <functional>
#include <thread>
#include <vector>

using namespace deck_test;

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
    ASSERT_TRUE (pumpUntilBuffered (deck, 1000)) << "decoder did not buffer in time";
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
    ASSERT_TRUE (pumpUntilBuffered (deck, 512)) << "decoder did not buffer in time";
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
    ASSERT_TRUE (pumpUntilBuffered (deck, 2048)) << "decoder did not buffer in time";
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
    ASSERT_TRUE (pumpUntilBuffered (deck, 512)) << "decoder did not buffer in time";
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
    ASSERT_TRUE (pumpUntilBuffered (deck, 2048, 4000)) << "decoder did not buffer in time";
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
    ASSERT_TRUE (pumpUntilBuffered (deck, 512)) << "decoder did not buffer in time";
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
