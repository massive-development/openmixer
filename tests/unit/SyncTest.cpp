// OpenMixer SyncTest - beat grid, quantizer, slip tracker and tempo/phase sync behaviour
#include <JuceHeader.h>
#include "engine/Deck.h"
#include "engine/Quantizer.h"
#include "engine/SlipTracker.h"
#include "engine/Sync.h"

#include <gtest/gtest.h>

#include <cmath>
#include <memory>

using namespace openmixer;

namespace
{
constexpr double kRate = 44100.0;

// Stands in for a Wave 3 analysis grid: 3 beats per bar, tempo from a table.
struct MockGrid final : BeatGrid
{
    MockGrid (double bpmValue, std::int64_t offset) : bpm (bpmValue), origin (offset) {}
    double getBpm() const noexcept override { return bpm; }
    int getBeatsPerBar() const noexcept override { return 3; }
    double beatAtFrame (std::int64_t f) const noexcept override { return static_cast<double> (f - origin) * bpm / (60.0 * kRate); }
    std::int64_t frameAtBeat (double b) const noexcept override { return origin + static_cast<std::int64_t> (std::llround (b * 60.0 * kRate / bpm)); }
    double bpm;
    std::int64_t origin;
};

// Advances two virtual decks the way the audio thread would and lets Sync steer the follower.
struct Sim
{
    std::vector<SyncDeckState> decks { { 0, 1.0, true }, { 0, 1.0, true } };

    void run (const Sync& sync, double seconds)
    {
        constexpr double dt = 0.05;
        for (double t = 0.0; t < seconds; t += dt)
        {
            const auto rates = sync.step (decks);
            for (size_t i = 0; i < decks.size(); ++i)
            {
                if (rates[i].has_value())
                    decks[i].rate = *rates[i];
                decks[i].position += static_cast<std::int64_t> (decks[i].rate * kRate * dt);
            }
        }
    }
};
}

TEST (BeatGridTest, FixedGridMapsFramesAndBeats)
{
    const FixedTempoGrid grid (120.0, kRate, 1000);
    EXPECT_EQ (grid.frameAtBeat (2.0), std::int64_t { 1000 + 44100 });
    EXPECT_NEAR (grid.beatAtFrame (1000 + 22050), 1.0, 1e-9);
    EXPECT_NEAR (grid.beatAtFrame (0), -1000.0 / 22050.0, 1e-9);
}

TEST (QuantizerTest, SnapsToBeatsFractionsAndBarsAndPassesThroughWithoutAGrid)
{
    const FixedTempoGrid grid (120.0, kRate, 1000);
    EXPECT_EQ (snapToNearest (&grid, 1000 + 22050 + 3000, 1.0), std::int64_t { 1000 + 22050 });
    EXPECT_EQ (snapToNearest (&grid, 1000 + 22050 + 12000, 1.0), std::int64_t { 1000 + 44100 });
    EXPECT_EQ (snapToPrevious (&grid, 1000 + 22050 + 12000, 1.0), std::int64_t { 1000 + 22050 });
    EXPECT_EQ (snapToNearest (&grid, 1000 + 6000, 0.25), std::int64_t { 1000 + 5512 + 1 });   // 1/4 beat = 5512.5
    EXPECT_EQ (snapToPrevious (&grid, 1000 + 80000, 4.0), std::int64_t { 1000 });
    EXPECT_EQ (snapToNearest (nullptr, 12345, 1.0), std::int64_t { 12345 });
    EXPECT_EQ (snapToNearest (&grid, 12345, 0.0), std::int64_t { 12345 });
}

TEST (QuantizerTest, QuantizedLoopStartsOnTheGridAndSpansTheRequestedBeats)
{
    const FixedTempoGrid grid (120.0, kRate, 1000);
    const auto loop = makeQuantizedLoop (&grid, 1000 + 22050 + 900, 4.0, 1.0);
    EXPECT_TRUE (loop.valid());
    EXPECT_EQ (loop.start, std::int64_t { 1000 + 22050 });
    EXPECT_EQ (loop.end - loop.start, std::int64_t { 4 * 22050 });
    EXPECT_FALSE (makeQuantizedLoop (&grid, 0, 0.0, 1.0).valid());
    EXPECT_FALSE (makeQuantizedLoop (nullptr, 0, 4.0, 1.0).valid());
}

TEST (SlipTrackerTest, ResumesAtTheGhostPosition)
{
    SlipTracker slip;
    EXPECT_EQ (slip.end (100000), std::int64_t { -1 });
    slip.begin (1000);
    slip.advance (500.0);
    slip.advance (500.0);
    EXPECT_TRUE (slip.isActive());
    EXPECT_EQ (slip.end (100000), std::int64_t { 2000 });
    EXPECT_FALSE (slip.isActive());
    slip.begin (90000);
    slip.advance (50000.0);
    EXPECT_EQ (slip.end (100000), std::int64_t { 99999 });
}

TEST (SyncTest, FollowerConvergesToTheLeadersTempoAndBeatPhase)
{
    Sync sync (2);
    sync.setGrid (0, std::make_shared<FixedTempoGrid> (120.0, kRate));
    sync.setGrid (1, std::make_shared<MockGrid> (126.0, 9000));
    sync.setLeader (0);
    sync.setEnabled (1, true);

    Sim sim;
    EXPECT_GT (std::abs (sync.phaseError (sim.decks, 1)), 0.05);
    sim.run (sync, 30.0);
    EXPECT_NEAR (sim.decks[1].rate, 120.0 / 126.0, 0.01);
    EXPECT_LT (std::abs (sync.phaseError (sim.decks, 1)), 0.02);
}

TEST (SyncTest, BarModeClosesWholeBeatOffsets)
{
    Sync sync (2);
    sync.setGrid (0, std::make_shared<MockGrid> (120.0, 0));
    sync.setGrid (1, std::make_shared<MockGrid> (120.0, 22050));   // one beat off
    sync.setLeader (0);
    sync.setEnabled (1, true);
    sync.setMode (SyncMode::Bar);

    Sim sim;
    EXPECT_NEAR (std::abs (sync.phaseError (sim.decks, 1)), 1.0, 1e-6);
    sim.run (sync, 40.0);
    EXPECT_LT (std::abs (sync.phaseError (sim.decks, 1)), 0.03);
}

TEST (SyncTest, NothingChangesWhenSyncCannotApply)
{
    Sync sync (3);
    Sim sim;
    sim.decks.push_back ({});
    sync.setGrid (0, std::make_shared<FixedTempoGrid> (120.0, kRate));
    sync.setGrid (1, std::make_shared<FixedTempoGrid> (100.0, kRate));
    sync.setLeader (0);

    EXPECT_FALSE (sync.step (sim.decks)[1].has_value());   // not enabled
    sync.setEnabled (1, true);
    sync.setEnabled (0, true);
    EXPECT_FALSE (sync.step (sim.decks)[0].has_value());   // leader never follows itself
    EXPECT_TRUE (sync.step (sim.decks)[1].has_value());
    sim.decks[0].playing = false;
    EXPECT_FALSE (sync.step (sim.decks)[1].has_value());   // leader stopped
    sim.decks[0].playing = true;
    sync.setGrid (1, nullptr);
    EXPECT_FALSE (sync.step (sim.decks)[1].has_value());   // no grid
    EXPECT_EQ (sync.phaseError (sim.decks, 1), 0.0);
    sync.setEnabled (7, true);                              // out of range ignored
    EXPECT_FALSE (sync.isEnabled (7));
}

TEST (SyncTest, AStoppedFollowerOnlyGetsTheTempoMatch)
{
    Sync sync (2);
    sync.setGrid (0, std::make_shared<FixedTempoGrid> (120.0, kRate));
    sync.setGrid (1, std::make_shared<FixedTempoGrid> (100.0, kRate));
    sync.setLeader (0);
    sync.setEnabled (1, true);
    Sim sim;
    sim.decks[1].playing = false;
    sim.decks[1].position = 5000;
    const auto rate = sync.step (sim.decks)[1];
    ASSERT_TRUE (rate.has_value());
    EXPECT_NEAR (*rate, 1.2, 1e-9);
}

TEST (SyncTest, ParameterTargetIdsAndDeckIntegration)
{
    Sync sync (2);
    EXPECT_TRUE (sync.setParameter ("leader", 0));
    EXPECT_EQ (sync.getLeader(), 0);
    EXPECT_TRUE (sync.setParameter ("1.sync", 1.0));
    EXPECT_TRUE (sync.isEnabled (1));
    EXPECT_TRUE (sync.setParameter ("1.sync", 0.0));
    EXPECT_FALSE (sync.isEnabled (1));
    EXPECT_TRUE (sync.setParameter ("mode", 1.0));
    EXPECT_FALSE (sync.setParameter ("nope", 1.0));
    EXPECT_FALSE (sync.setParameter ("x.sync", 1.0));

    sync.setGrid (0, std::make_shared<FixedTempoGrid> (120.0, kRate));
    sync.setGrid (1, std::make_shared<FixedTempoGrid> (96.0, kRate));
    sync.setEnabled (1, true);
    Deck leader, follower;
    leader.play();
    leader.setRate (1.0);
    sync.apply ({ &leader, &follower, nullptr });
    EXPECT_NEAR (follower.getRate(), 1.25, 1e-9);
    EXPECT_EQ (leader.getRate(), 1.0);
}
