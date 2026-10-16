#include <JuceHeader.h>
#include "engine/Deck.h"
#include "engine/Quantizer.h"
#include "engine/SlipTracker.h"
#include "engine/Sync.h"

#include <cmath>
#include <memory>

namespace OpenMixerTests
{
    class SyncTest : public juce::UnitTest
    {
    public:
        SyncTest() : juce::UnitTest ("Sync and quantize", "OpenMixer Engine") {}

        static constexpr double kRate = 44100.0;

        // Stands in for a Wave 3 analysis grid: 3 beats per bar, tempo from a table.
        struct MockGrid final : openmixer::BeatGrid
        {
            MockGrid (double bpmValue, std::int64_t offset) : bpm (bpmValue), origin (offset) {}
            double getBpm() const noexcept override { return bpm; }
            int getBeatsPerBar() const noexcept override { return 3; }
            double beatAtFrame (std::int64_t f) const noexcept override { return (double) (f - origin) * bpm / (60.0 * kRate); }
            std::int64_t frameAtBeat (double b) const noexcept override { return origin + (std::int64_t) std::llround (b * 60.0 * kRate / bpm); }
            double bpm;
            std::int64_t origin;
        };

        // Advances two virtual decks the way the audio thread would and lets Sync steer the follower.
        struct Sim
        {
            std::vector<openmixer::SyncDeckState> decks { { 0, 1.0, true }, { 0, 1.0, true } };

            void run (const openmixer::Sync& sync, double seconds)
            {
                constexpr double dt = 0.05;
                for (double t = 0.0; t < seconds; t += dt)
                {
                    const auto rates = sync.step (decks);
                    for (size_t i = 0; i < decks.size(); ++i)
                    {
                        if (rates[i].has_value())
                            decks[i].rate = *rates[i];
                        decks[i].position += (std::int64_t) (decks[i].rate * kRate * dt);
                    }
                }
            }
        };

        void gridAndQuantizer()
        {
            using namespace openmixer;
            beginTest ("Fixed grid maps frames and beats");
            const FixedTempoGrid grid (120.0, kRate, 1000);
            expectEquals (grid.frameAtBeat (2.0), (std::int64_t) 1000 + 44100);
            expectWithinAbsoluteError (grid.beatAtFrame (1000 + 22050), 1.0, 1e-9);
            expectWithinAbsoluteError (grid.beatAtFrame (0), -1000.0 / 22050.0, 1e-9);

            beginTest ("Quantizer snaps to beats, fractions and bars; passes through without a grid");
            expectEquals (snapToNearest (&grid, 1000 + 22050 + 3000, 1.0), (std::int64_t) 1000 + 22050);
            expectEquals (snapToNearest (&grid, 1000 + 22050 + 12000, 1.0), (std::int64_t) 1000 + 44100);
            expectEquals (snapToPrevious (&grid, 1000 + 22050 + 12000, 1.0), (std::int64_t) 1000 + 22050);
            expectEquals (snapToNearest (&grid, 1000 + 6000, 0.25), (std::int64_t) 1000 + 5512 + 1);   // 1/4 beat = 5512.5
            expectEquals (snapToPrevious (&grid, 1000 + 80000, 4.0), (std::int64_t) 1000);
            expectEquals (snapToNearest (nullptr, 12345, 1.0), (std::int64_t) 12345);
            expectEquals (snapToNearest (&grid, 12345, 0.0), (std::int64_t) 12345);

            beginTest ("Quantized loop starts on the grid and spans the requested beats");
            const auto loop = makeQuantizedLoop (&grid, 1000 + 22050 + 900, 4.0, 1.0);
            expect (loop.valid());
            expectEquals (loop.start, (std::int64_t) 1000 + 22050);
            expectEquals (loop.end - loop.start, (std::int64_t) 4 * 22050);
            expect (! makeQuantizedLoop (&grid, 0, 0.0, 1.0).valid());
            expect (! makeQuantizedLoop (nullptr, 0, 4.0, 1.0).valid());

            beginTest ("Slip tracker resumes at the ghost position");
            SlipTracker slip;
            expectEquals (slip.end (100000), (std::int64_t) -1);
            slip.begin (1000);
            slip.advance (500.0);
            slip.advance (500.0);
            expect (slip.isActive());
            expectEquals (slip.end (100000), (std::int64_t) 2000);
            expect (! slip.isActive());
            slip.begin (90000);
            slip.advance (50000.0);
            expectEquals (slip.end (100000), (std::int64_t) 99999);
        }

        void syncBehaviour()
        {
            using namespace openmixer;
            beginTest ("Follower converges to the leader's tempo and beat phase");
            {
                Sync sync (2);
                sync.setGrid (0, std::make_shared<FixedTempoGrid> (120.0, kRate));
                sync.setGrid (1, std::make_shared<MockGrid> (126.0, 9000));
                sync.setLeader (0);
                sync.setEnabled (1, true);

                Sim sim;
                expectGreaterThan (std::abs (sync.phaseError (sim.decks, 1)), 0.05);
                sim.run (sync, 30.0);
                expectWithinAbsoluteError (sim.decks[1].rate, 120.0 / 126.0, 0.01);
                expectLessThan (std::abs (sync.phaseError (sim.decks, 1)), 0.02);
            }

            beginTest ("Bar mode closes whole-beat offsets");
            {
                Sync sync (2);
                sync.setGrid (0, std::make_shared<MockGrid> (120.0, 0));
                sync.setGrid (1, std::make_shared<MockGrid> (120.0, 22050));   // one beat off
                sync.setLeader (0);
                sync.setEnabled (1, true);
                sync.setMode (SyncMode::Bar);

                Sim sim;
                expectWithinAbsoluteError (std::abs (sync.phaseError (sim.decks, 1)), 1.0, 1e-6);
                sim.run (sync, 40.0);
                expectLessThan (std::abs (sync.phaseError (sim.decks, 1)), 0.03);
            }

            beginTest ("Nothing changes when sync cannot apply");
            {
                Sync sync (3);
                Sim sim;
                sim.decks.push_back ({});
                sync.setGrid (0, std::make_shared<FixedTempoGrid> (120.0, kRate));
                sync.setGrid (1, std::make_shared<FixedTempoGrid> (100.0, kRate));
                sync.setLeader (0);

                expect (! sync.step (sim.decks)[1].has_value());   // not enabled
                sync.setEnabled (1, true);
                sync.setEnabled (0, true);
                expect (! sync.step (sim.decks)[0].has_value());   // leader never follows itself
                expect (sync.step (sim.decks)[1].has_value());
                sim.decks[0].playing = false;
                expect (! sync.step (sim.decks)[1].has_value());   // leader stopped
                sim.decks[0].playing = true;
                sync.setGrid (1, nullptr);
                expect (! sync.step (sim.decks)[1].has_value());   // no grid
                expectEquals (sync.phaseError (sim.decks, 1), 0.0);
                sync.setEnabled (7, true);                          // out of range ignored
                expect (! sync.isEnabled (7));
            }

            beginTest ("A stopped follower only gets the tempo match");
            {
                Sync sync (2);
                sync.setGrid (0, std::make_shared<FixedTempoGrid> (120.0, kRate));
                sync.setGrid (1, std::make_shared<FixedTempoGrid> (100.0, kRate));
                sync.setLeader (0);
                sync.setEnabled (1, true);
                Sim sim;
                sim.decks[1].playing = false;
                sim.decks[1].position = 5000;
                expectWithinAbsoluteError (*sync.step (sim.decks)[1], 1.2, 1e-9);
            }
        }

        void controlSurface()
        {
            using namespace openmixer;
            beginTest ("ParameterTarget ids and Deck integration");
            Sync sync (2);
            expect (sync.setParameter ("leader", 0));
            expectEquals (sync.getLeader(), 0);
            expect (sync.setParameter ("1.sync", 1.0));
            expect (sync.isEnabled (1));
            expect (sync.setParameter ("1.sync", 0.0));
            expect (! sync.isEnabled (1));
            expect (sync.setParameter ("mode", 1.0));
            expect (! sync.setParameter ("nope", 1.0));
            expect (! sync.setParameter ("x.sync", 1.0));

            sync.setGrid (0, std::make_shared<FixedTempoGrid> (120.0, kRate));
            sync.setGrid (1, std::make_shared<FixedTempoGrid> (96.0, kRate));
            sync.setEnabled (1, true);
            Deck leader, follower;
            leader.play();
            leader.setRate (1.0);
            sync.apply ({ &leader, &follower, nullptr });
            expectWithinAbsoluteError (follower.getRate(), 1.25, 1e-9);
            expectEquals (leader.getRate(), 1.0);
        }

        void runTest() override
        {
            gridAndQuantizer();
            syncBehaviour();
            controlSurface();
        }
    };

    static SyncTest syncTest;
}
