// OpenMixer EngineWiringTest - FX racks, sync, quantized loops and slip reached through the router
#include <JuceHeader.h>
#include "controllers/ParameterRouter.h"
#include "engine/DjEngine.h"
#include "engine/MockAudioDevice.h"

#include <gtest/gtest.h>

#include <cmath>
#include <functional>
#include <memory>
#include <thread>

using namespace openmixer;

namespace
{
constexpr double kRate = 44100.0;
constexpr int kFrames = 44100 * 10;

juce::File writeSine (const juce::File& f)
{
    f.deleteFile();
    juce::WavAudioFormat wav;
    std::unique_ptr<juce::FileOutputStream> stream (f.createOutputStream());
    std::unique_ptr<juce::AudioFormatWriter> writer (wav.createWriterFor (stream.get(), kRate, 2, 24, {}, 0));
    stream.release();   // writer owns the stream

    juce::AudioBuffer<float> buf (2, kFrames);
    for (int i = 0; i < kFrames; ++i)
        for (int c = 0; c < 2; ++c)
            buf.setSample (c, i, 0.3f * std::sin (2.0f * juce::MathConstants<float>::pi * 220.0f * static_cast<float> (i) / static_cast<float> (kRate)));
    writer->writeFromAudioSampleBuffer (buf, 0, kFrames);
    return f;
}

bool waitFor (const std::function<bool()>& cond)
{
    const auto end = juce::Time::getMillisecondCounter() + 20000;   // generous: valgrind runs ~30x slower
    while (! cond())
    {
        if (juce::Time::getMillisecondCounter() > end) return false;
        std::this_thread::yield();
    }
    return true;
}

// Services a paused deck's seek handshake until it sits on the target.
bool settleAt (Deck& deck, juce::int64 target)
{
    float l[64], r[64];
    return waitFor ([&] { deck.renderStereo (l, r, 64); return deck.getPositionFrames() == target; });
}

// Services the handshake of a paused deck until its ring holds enough to start glitch-free.
bool buffered (Deck& deck)
{
    float l[64], r[64];
    deck.renderStereo (l, r, 64);
    return deck.getBufferedFrames() > 8192;
}

class WiringFixture : public ::testing::Test
{
protected:
    void SetUp() override
    {
        dir = juce::File::getSpecialLocation (juce::File::tempDirectory)
                  .getChildFile ("openmixer_wiring_test_" + juce::Uuid().toString());   // unique per process: ctest -j
        ASSERT_TRUE (dir.createDirectory());
        file = writeSine (dir.getChildFile ("sine.wav"));
        router.registerTarget ("fx", &engine.getFx());
        router.registerTarget ("sync", &engine.getSync());
        for (int i = 0; i < engine.getNumDecks(); ++i)
            router.registerTarget ("deck." + juce::String (i), &engine.getDeckControl (i));
    }

    void TearDown() override { dir.deleteRecursively(); }

    DjEngine engine;
    ParameterRouter router;
    juce::File dir, file;
};
}

using EngineWiringTest = WiringFixture;
using EngineWiringSlow = WiringFixture;

TEST_F (EngineWiringTest, RouterReachesMasterAndChannelFxParameters)
{
    EXPECT_TRUE (router.setParameter ("fx.master.fx1.mix", 0.3));
    EXPECT_FLOAT_EQ (engine.getFx().getMasterRack().getUnit (1)->getMix(), 0.3f);
    EXPECT_TRUE (router.setParameter ("fx.ch1.fx2.enable", 1.0));
    EXPECT_TRUE (engine.getFx().getChannelRack (1).getUnit (2)->isEnabled());
    EXPECT_FALSE (engine.getFx().getChannelRack (0).getUnit (2)->isEnabled()) << "channels are independent";
    EXPECT_TRUE (router.setParameter ("fx.ch0.bpm", 140.0));
    EXPECT_DOUBLE_EQ (engine.getFx().getChannelRack (0).getUnit (0)->getTempo(), 140.0);
}

TEST_F (EngineWiringTest, RouterRejectsUnknownFxIds)
{
    for (const char* id : { "fx.ch9.fx0.mix", "fx.master.fx99.mix", "fx.bus.fx0.mix", "fx.master", "fx.ch.fx0.mix", "fx.master.nope" })
        EXPECT_FALSE (router.setParameter (id, 0.5)) << id;
}

TEST_F (EngineWiringTest, RacksSitInTheSignalPath)
{
    router.setParameter ("fx.master.fx0.enable", 1.0);
    router.setParameter ("fx.ch1.fx3.enable", 1.0);
    MockAudioDevice dev (kRate, 256, 0, 2);
    dev.start (engine);
    dev.renderBlocks (2);
    EXPECT_TRUE (engine.getFx().getMasterRack().getUnit (0)->isActive());
    EXPECT_TRUE (engine.getFx().getChannelRack (1).getUnit (3)->isActive());
    EXPECT_FALSE (engine.getFx().getChannelRack (0).getUnit (3)->isActive());
}

TEST_F (EngineWiringTest, FxTempoFollowsDeckGridAndRate)
{
    engine.setGrid (0, std::make_shared<FixedTempoGrid> (120.0, kRate));
    engine.setGrid (1, std::make_shared<FixedTempoGrid> (100.0, kRate));
    engine.getDeck (0).setRate (1.1);
    engine.updateControl (0.03);
    EXPECT_NEAR (engine.getFx().getChannelRack (0).getUnit (0)->getTempo(), 132.0, 1e-9);
    EXPECT_NEAR (engine.getFx().getChannelRack (1).getUnit (0)->getTempo(), 100.0, 1e-9);
    EXPECT_NEAR (engine.getFx().getMasterRack().getUnit (0)->getTempo(), 132.0, 1e-9) << "master follows first deck with a grid";

    router.setParameter ("deck.1.sync.master", 1.0);
    engine.updateControl (0.03);
    EXPECT_NEAR (engine.getFx().getMasterRack().getUnit (0)->getTempo(), 100.0, 1e-9) << "master follows the sync leader";
}

TEST_F (EngineWiringTest, SyncIdsSelectLeaderAndFollowerAndMatchTempo)
{
    engine.setGrid (0, std::make_shared<FixedTempoGrid> (120.0, kRate));
    engine.setGrid (1, std::make_shared<FixedTempoGrid> (100.0, kRate));
    EXPECT_TRUE (router.setParameter ("deck.0.sync.master", 1.0));
    EXPECT_TRUE (router.setParameter ("deck.1.sync", 1.0));
    EXPECT_EQ (engine.getSync().getLeader(), 0);
    EXPECT_TRUE (engine.getSync().isEnabled (1));

    engine.getDeck (0).play();
    engine.getDeck (1).play();   // no audio callback: positions stay at 0, so phase is aligned
    engine.updateControl (0.03);
    EXPECT_NEAR (engine.getDeck (1).getRate(), 1.2, 1e-9) << "100 bpm deck sped up to the 120 bpm leader";
    EXPECT_DOUBLE_EQ (engine.getDeck (0).getRate(), 1.0);

    EXPECT_TRUE (router.setParameter ("deck.1.sync", 0.0));
    EXPECT_TRUE (router.setParameter ("deck.1.sync.master", 0.0)) << "releasing a non-leader is a no-op";
    EXPECT_EQ (engine.getSync().getLeader(), 0);
    router.setParameter ("deck.0.sync.master", 0.0);
    EXPECT_EQ (engine.getSync().getLeader(), -1);
}

TEST_F (EngineWiringTest, DeckIdsStillReachTheDeckAndUnknownIdsFail)
{
    EXPECT_TRUE (router.setParameter ("deck.0.rate", 1.05));
    EXPECT_DOUBLE_EQ (engine.getDeck (0).getRate(), 1.05);
    EXPECT_FALSE (router.setParameter ("deck.0.bogus", 1.0));
    for (const char* id : { "deck.0.loop.quantized.abc", "deck.0.loop.quantized.", "deck.0.loop.quantized.0", "deck.0.loop.quantized.1.2.3", "deck.0.loop.quantized..5" })
        EXPECT_FALSE (router.setParameter (id, 1.0)) << id;
    EXPECT_FALSE (router.setParameter ("deck.0.loop.quantized.2", 1.0)) << "no grid: press reports failure";
}

TEST_F (EngineWiringSlow, SyncAlignsTwoDecksOnFixedGrids)
{
    ASSERT_TRUE (engine.getDeck (0).load (file));
    ASSERT_TRUE (engine.getDeck (1).load (file));
    engine.setGrid (0, std::make_shared<FixedTempoGrid> (120.0, kRate));
    engine.setGrid (1, std::make_shared<FixedTempoGrid> (120.0, kRate));
    MockAudioDevice dev (kRate, 256, 0, 2);
    dev.start (engine);
    engine.getDeck (1).seek (2205);   // follower starts 0.1 beat late
    ASSERT_TRUE (settleAt (engine.getDeck (1), 2205));
    ASSERT_TRUE (waitFor ([&] { return buffered (engine.getDeck (0)) && buffered (engine.getDeck (1)); }));   // no start-up underruns skewing the offset
    router.setParameter ("deck.0.sync.master", 1.0);
    router.setParameter ("deck.1.sync", 1.0);
    engine.getDeck (0).play();
    engine.getDeck (1).play();

    for (int block = 0; block < 1200; ++block)
    {
        waitFor ([&] { return engine.getDeck (0).getBufferedFrames() >= 2048 && engine.getDeck (1).getBufferedFrames() >= 2048; });   // keep the unpaced pump from underrunning (slow under valgrind)
        dev.renderBlock();
        if (block % 8 == 0)
            engine.updateControl (0.046);
        std::this_thread::yield();
    }
    const auto states = std::vector<SyncDeckState> { { engine.getDeck (0).getPositionFrames(), engine.getDeck (0).getRate(), true },
                                                     { engine.getDeck (1).getPositionFrames(), engine.getDeck (1).getRate(), true } };
    EXPECT_LT (std::abs (engine.getSync().phaseError (states, 1)), 0.02) << "follower converged onto the leader's beat";
}

TEST_F (EngineWiringSlow, QuantizedLoopBoundsComeFromTheGrid)
{
    ASSERT_TRUE (engine.getDeck (0).load (file));
    engine.getDeck (0).seek (30000);   // beat 1.36 at 120 bpm: nearest beat is 22050
    ASSERT_TRUE (settleAt (engine.getDeck (0), 30000));

    EXPECT_FALSE (engine.getDeckControl (0).setQuantizedLoop (2.0)) << "no grid yet";
    engine.setGrid (0, std::make_shared<FixedTempoGrid> (120.0, kRate));
    EXPECT_TRUE (router.setParameter ("deck.0.loop.quantized.2", 1.0));
    const auto loop = engine.getDeck (0).getLoop();
    EXPECT_EQ (loop.start, 22050);
    EXPECT_EQ (loop.end, 22050 + 44100);

    EXPECT_TRUE (router.setParameter ("deck.0.loop.quantized.0.5", 1.0));
    EXPECT_EQ (engine.getDeck (0).getLoop().end - engine.getDeck (0).getLoop().start, 11025) << "half-beat loop";
}

TEST_F (EngineWiringSlow, SlipResumesWhereTheTrackWouldHaveBeen)
{
    ASSERT_TRUE (engine.getDeck (0).load (file));
    engine.getDeck (0).seek (1000);
    ASSERT_TRUE (settleAt (engine.getDeck (0), 1000));

    EXPECT_TRUE (router.setParameter ("deck.0.slip", 1.0));
    EXPECT_TRUE (engine.getDeckControl (0).isSlipping());
    engine.getDeck (0).play();   // no audio callback runs: the playhead is held, the ghost moves
    engine.updateControl (0.5);
    engine.getDeck (0).pause();
    EXPECT_TRUE (router.setParameter ("deck.0.slip", 0.0));
    EXPECT_FALSE (engine.getDeckControl (0).isSlipping());
    EXPECT_TRUE (settleAt (engine.getDeck (0), 1000 + 22050));
}
