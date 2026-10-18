// OpenMixer DeckLoopTest - gapless loop wrap, loop exit, long loops past the prefetch slot, hotcues
#include "DeckTestUtil.h"

#include <algorithm>
#include <chrono>
#include <cmath>
#include <vector>

using namespace deck_test;

namespace
{
using DeckLoopTest = DeckFixture;
using DeckLoopSlow = DeckFixture;

int frameIndex (float sample) { return static_cast<int> (std::lround (sample * static_cast<float> (kFrames))); }

// Renders device-paced 64-frame blocks and returns the source frame index of every output sample.
std::vector<int> renderIndices (openmixer::Deck& deck, int numBlocks, const std::function<void (int)>& beforeBlock = {})
{
    std::vector<int> out;
    std::vector<float> l (64), r (64);
    for (int b = 0; b < numBlocks; ++b)
    {
        if (beforeBlock)
            beforeBlock (b);
        deck.renderStereo (l.data(), r.data(), 64);
        for (auto v : l)
            out.push_back (frameIndex (v));
        std::this_thread::sleep_for (std::chrono::milliseconds (1));
    }
    return out;
}

void startAtZero (openmixer::Deck& deck, const juce::File& file)
{
    deck.prepare (kRate, 64);
    ASSERT_TRUE (deck.load (file));
    ASSERT_TRUE (pumpUntilBuffered (deck, 4096));
}

void expectSequence (const std::vector<int>& got, const std::function<int (int)>& expected, size_t count)
{
    ASSERT_GE (got.size(), count);
    for (size_t i = 0; i < count; ++i)
        if (got[i] != expected (static_cast<int> (i)))
        {
            ADD_FAILURE() << "sample " << i << ": got frame " << got[i] << ", expected " << expected (static_cast<int> (i));
            return;
        }
}
}

TEST_F (DeckLoopTest, SetLoopValidatesAndReportsState)
{
    openmixer::Deck deck;
    ASSERT_TRUE (deck.load (wavFile));
    EXPECT_FALSE (deck.setLoop ({ 100, 101 }));
    EXPECT_FALSE (deck.isLooping());
    ASSERT_TRUE (deck.setLoop ({ 100, kFrames + 500 }));
    EXPECT_EQ (deck.getLoop().end, kFrames);   // clamped to the track
    EXPECT_TRUE (deck.isLooping());
    deck.exitLoop();
    EXPECT_FALSE (deck.isLooping());
    EXPECT_TRUE (deck.getLoop().valid());
    deck.reloop();
    EXPECT_TRUE (deck.isLooping());
    ASSERT_TRUE (deck.load (wavFile));
    EXPECT_FALSE (deck.getLoop().valid()) << "a new track starts without a loop";
}

TEST_F (DeckLoopSlow, LoopWrapsSampleAccurateWithoutUnderruns)
{
    openmixer::Deck deck;
    startAtZero (deck, wavFile);
    ASSERT_TRUE (deck.setLoop ({ 1000, 1500 }));
    ASSERT_TRUE (waitFor ([&] { return deck.isLoopPrefetched(); }));
    deck.play();
    const auto got = renderIndices (deck, 60);
    expectSequence (got, [] (int i) { return i < 1500 ? i : 1000 + (i - 1500) % 500; }, got.size());
    EXPECT_EQ (deck.getUnderrunCount(), 0U);
}

TEST_F (DeckLoopSlow, LoopLongerThanThePrefetchSlotFallsBackToTheRing)
{
    openmixer::Deck deck (1U << 16, 2048);
    startAtZero (deck, wavFile);
    ASSERT_TRUE (deck.setLoop ({ 1000, 9000 }));
    ASSERT_TRUE (waitFor ([&] { return deck.isLoopPrefetched(); }));
    deck.play();
    const auto got = renderIndices (deck, 400);
    expectSequence (got, [] (int i) { return i < 9000 ? i : 1000 + (i - 9000) % 8000; }, got.size());
    EXPECT_EQ (deck.getUnderrunCount(), 0U);
}

TEST_F (DeckLoopSlow, ExitingALoopContinuesLinearly)
{
    openmixer::Deck deck;
    startAtZero (deck, wavFile);
    ASSERT_TRUE (deck.setLoop ({ 1000, 1500 }));
    ASSERT_TRUE (waitFor ([&] { return deck.isLoopPrefetched(); }));
    deck.play();
    const auto got = renderIndices (deck, 120, [&] (int block) { if (block == 40) deck.exitLoop(); });

    int wraps = 0;
    for (size_t i = 1; i < got.size(); ++i)
    {
        const bool wrap = got[i - 1] == 1499 && got[i] == 1000;
        wraps += wrap ? 1 : 0;
        if (! wrap && got[i] != got[i - 1] + 1)
        {
            ADD_FAILURE() << "discontinuity at sample " << i << ": " << got[i - 1] << " -> " << got[i];
            break;
        }
    }
    EXPECT_GE (wraps, 2);
    EXPECT_GT (got.back(), 1500) << "playback should leave the loop";
    EXPECT_EQ (deck.getUnderrunCount(), 0U);
}

TEST_F (DeckLoopTest, HotcueParametersSetJumpAndClear)
{
    openmixer::Deck deck;
    ASSERT_TRUE (deck.load (wavFile));
    deck.setHotcue (2, 5000);
    EXPECT_EQ (deck.getHotcue (2).value_or (-1), 5000);
    EXPECT_TRUE (deck.setParameter ("hotcue2.clear", 1.0));
    EXPECT_FALSE (deck.getHotcue (2).has_value());
    EXPECT_TRUE (deck.setParameter ("hotcue3", 1.0));   // empty: stores the playhead
    EXPECT_EQ (deck.getHotcue (3).value_or (-1), 0);
    EXPECT_FALSE (deck.setParameter ("hotcue8", 1.0));
    EXPECT_FALSE (deck.setParameter ("hotcuex", 1.0));
    EXPECT_FALSE (deck.setParameter ("hotcue1.bogus", 1.0));
    EXPECT_TRUE (deck.setParameter ("loop.exit", 1.0));
}

TEST_F (DeckLoopTest, SeekingOutsideALoopExitsIt)
{
    openmixer::Deck deck;
    ASSERT_TRUE (deck.load (wavFile));
    ASSERT_TRUE (deck.setLoop ({ 1000, 2000 }));
    deck.seek (1500);
    EXPECT_TRUE (deck.isLooping());
    deck.seek (5000);
    EXPECT_FALSE (deck.isLooping());
}

TEST_F (DeckLoopSlow, HotcueJumpRepositionsPlayback)
{
    openmixer::Deck deck;
    startAtZero (deck, wavFile);
    deck.setHotcue (0, 12000);
    deck.triggerHotcue (0);
    std::vector<float> l (64), r (64);
    ASSERT_TRUE (waitFor ([&]
    {
        deck.renderStereo (l.data(), r.data(), 64);
        return deck.getPositionFrames() == 12000 && deck.getBufferedFrames() > 256;
    }));
    deck.play();
    deck.renderStereo (l.data(), r.data(), 64);
    EXPECT_EQ (frameIndex (l[0]), 12000);
}

TEST_F (DeckLoopSlow, ResizingALoopWhileLoopingStaysContinuous)
{
    openmixer::Deck deck;
    startAtZero (deck, wavFile);
    ASSERT_TRUE (deck.setLoop ({ 1000, 1800 }));
    ASSERT_TRUE (waitFor ([&] { return deck.isLoopPrefetched(); }));
    deck.play();
    const auto got = renderIndices (deck, 100, [&] (int block) { if (block == 50) deck.setLoop ({ 1000, 1400 }); });
    for (size_t i = 1; i < got.size(); ++i)
    {
        const auto wrapsTo = [&] (int len) { return got[i] == 1000 + (got[i - 1] + 1 - 1000) % len; };
        const bool wrap = got[i - 1] >= 1399 && (wrapsTo (800) || wrapsTo (400));
        if (! wrap && got[i] != got[i - 1] + 1)
        {
            ADD_FAILURE() << "discontinuity at sample " << i << ": " << got[i - 1] << " -> " << got[i];
            break;
        }
    }
    EXPECT_LT (*std::max_element (got.end() - 2000, got.end()), 1400);
    EXPECT_EQ (deck.getUnderrunCount(), 0U);
}
