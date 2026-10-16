#include <JuceHeader.h>
#include "engine/Mixer.h"

#include <cmath>
#include <vector>

namespace OpenMixerTests
{
    class MixerTest : public juce::UnitTest
    {
    public:
        MixerTest() : juce::UnitTest ("Mixer", "OpenMixer Engine") {}

        static constexpr double sr = 48000.0;
        static constexpr int N = 16384;

        struct Rig
        {
            openmixer::Mixer mixer { 2 };
            std::vector<float> in[2][2];   // [channel][L/R]
            std::vector<float> mL, mR, cL, cR;

            Rig() : mL (N), mR (N), cL (N), cR (N)
            {
                mixer.prepare (sr, 512);
                for (auto& c : in) for (auto& v : c) v.assign (N, 0.0f);
            }

            void setSine (int ch, double hz, float amp = 0.5f)
            {
                for (int i = 0; i < N; ++i)
                {
                    const float s = amp * (float) std::sin (2.0 * juce::MathConstants<double>::pi * hz * i / sr);
                    in[ch][0][(size_t) i] = s;
                    in[ch][1][(size_t) i] = s;
                }
            }

            void run()
            {
                openmixer::Mixer::ChannelInput ci[2] = { { in[0][0].data(), in[0][1].data() },
                                                         { in[1][0].data(), in[1][1].data() } };
                for (int off = 0; off < N; off += 512)   // block-wise like a device
                {
                    openmixer::Mixer::ChannelInput blk[2];
                    for (int c = 0; c < 2; ++c) { blk[c].left = ci[c].left + off; blk[c].right = ci[c].right + off; }
                    mixer.process (blk, mL.data() + off, mR.data() + off, cL.data() + off, cR.data() + off, 512);
                }
            }

            static double rms (const std::vector<float>& v)
            {
                double s = 0; int cnt = 0;
                for (size_t i = N / 2; i < v.size(); ++i) { s += v[i] * v[i]; ++cnt; }
                return std::sqrt (s / cnt);
            }
        };

        static double dbOf (double ratio) { return 20.0 * std::log10 (std::max (ratio, 1e-12)); }

        /** Gain in dB through master for a sine at hz with the given setup. */
        template <typename Setup>
        double gainDb (double hz, Setup setup)
        {
            Rig r;
            r.setSine (0, hz);
            setup (r.mixer);
            r.run();
            return dbOf (Rig::rms (r.mL) / (0.5 / std::sqrt (2.0)));
        }

        void runTest() override
        {
            using namespace openmixer;

            beginTest ("Flat settings are unity gain");
            for (double hz : { 60.0, 1000.0, 10000.0 })
                expectWithinAbsoluteError (gainDb (hz, [] (Mixer&) {}), 0.0, 0.1);

            beginTest ("3-band EQ cuts and boosts the intended band");
            expect (gainDb (60.0,    [] (Mixer& m) { m.setEq (0, 0, -48.0f); }) < -30.0, "low kill");
            expectWithinAbsoluteError (gainDb (10000.0, [] (Mixer& m) { m.setEq (0, 0, -48.0f); }), 0.0, 0.5);
            expect (gainDb (1000.0,  [] (Mixer& m) { m.setEq (0, 1, -48.0f); }) < -30.0, "mid kill");
            expectWithinAbsoluteError (gainDb (60.0, [] (Mixer& m) { m.setEq (0, 1, -48.0f); }), 0.0, 5.0);
            expect (gainDb (12000.0, [] (Mixer& m) { m.setEq (0, 2, -48.0f); }) < -30.0, "high kill");
            expectWithinAbsoluteError (gainDb (100.0, [] (Mixer& m) { m.setEq (0, 2, -48.0f); }), 0.0, 0.5);
            expectWithinAbsoluteError (gainDb (60.0, [] (Mixer& m) { m.setEq (0, 0, 6.0f); }), 6.0, 1.0);
            expectWithinAbsoluteError (gainDb (1000.0, [] (Mixer& m) { m.setEq (0, 1, 6.0f); }), 6.0, 0.3);
            expectWithinAbsoluteError (gainDb (12000.0, [] (Mixer& m) { m.setEq (0, 2, 6.0f); }), 6.0, 1.0);

            beginTest ("Sweep filter: low-pass, high-pass and bypass");
            expect (gainDb (10000.0, [] (Mixer& m) { m.setFilter (0, -0.8f); }) < -20.0, "LP removes highs");
            expectWithinAbsoluteError (gainDb (40.0, [] (Mixer& m) { m.setFilter (0, -0.8f); }), 0.0, 1.0);
            expect (gainDb (100.0, [] (Mixer& m) { m.setFilter (0, 0.8f); }) < -20.0, "HP removes lows");
            expectWithinAbsoluteError (gainDb (10000.0, [] (Mixer& m) { m.setFilter (0, 0.8f); }), 0.0, 1.0);
            expectWithinAbsoluteError (gainDb (100.0, [] (Mixer& m) { m.setFilter (0, 0.01f); }), 0.0, 0.01);

            beginTest ("Channel fader and master gain scale output");
            expectWithinAbsoluteError (gainDb (1000.0, [] (Mixer& m) { m.setFader (0, 0.5f); }), -6.02, 0.1);
            expect (gainDb (1000.0, [] (Mixer& m) { m.setFader (0, 0.0f); }) < -100.0);
            expectWithinAbsoluteError (gainDb (1000.0, [] (Mixer& m) { m.setMasterGain (0.5f); }), -6.02, 0.1);
            expectWithinAbsoluteError (gainDb (1000.0, [] (Mixer& m) { m.setTrim (0, 2.0f); }), 6.02, 0.1);

            beginTest ("Parameters are clamped");
            {
                Mixer m (2);
                m.setFader (0, 5.0f);  expectEquals (m.getFader (0), 1.0f);
                m.setEq (0, 1, -999.0f); expectEquals (m.getEq (0, 1), Mixer::kMinEqDb);
                m.setFilter (1, -3.0f); expectEquals (m.getFilter (1), -1.0f);
                m.setFader (99, 0.25f); expectEquals (m.getFader (1), 0.25f);   // index clamps
            }

            beginTest ("Crossfader routes assigned channels");
            {
                for (auto curve : { CrossfaderCurve::Linear, CrossfaderCurve::ConstantPower })
                {
                    Rig r;
                    r.setSine (0, 1000.0); r.setSine (1, 3000.0);
                    r.mixer.setCrossfaderCurve (curve);
                    r.mixer.setCrossfaderAssign (0, CrossfaderAssign::Left);
                    r.mixer.setCrossfaderAssign (1, CrossfaderAssign::Right);

                    r.mixer.setCrossfader (0.0f);
                    r.run();
                    const double both = Rig::rms (r.mL);
                    expectWithinAbsoluteError (both, 0.5 / std::sqrt (2.0), 0.005);   // only ch0

                    r.mixer.setCrossfader (1.0f);
                    r.run();
                    expectWithinAbsoluteError (Rig::rms (r.mL), 0.5 / std::sqrt (2.0), 0.005);   // only ch1

                    r.mixer.setCrossfader (0.5f);
                    r.run();
                    // Two uncorrelated equal sines: power sums. Linear = 2*(0.5)^2, const-power = 2*(0.707)^2 * ... both per formula
                    const double g = curve == CrossfaderCurve::Linear ? 0.5 : std::sqrt (0.5);
                    const double expected = std::sqrt (2.0 * g * g) * 0.5 / std::sqrt (2.0);
                    expectWithinAbsoluteError (Rig::rms (r.mL), expected, 0.01);
                }
            }

            beginTest ("Thru channels ignore the crossfader");
            {
                Rig r;
                r.setSine (0, 1000.0);
                r.mixer.setCrossfader (1.0f);
                r.run();
                expectWithinAbsoluteError (Rig::rms (r.mL), 0.5 / std::sqrt (2.0), 0.005);
            }

            beginTest ("PFL cue bus is pre-fader and independent of crossfader");
            {
                Rig r;
                r.setSine (0, 1000.0); r.setSine (1, 3000.0);
                r.mixer.setCrossfaderAssign (0, CrossfaderAssign::Left);
                r.mixer.setCrossfader (1.0f);          // ch0 fully cut from master
                r.mixer.setFader (0, 0.0f);
                r.mixer.setPfl (0, true);
                r.run();
                const double unity = 0.5 / std::sqrt (2.0);
                expect (Rig::rms (r.cL) > unity * 0.98 && Rig::rms (r.cL) < unity * 1.02, "cue carries ch0 at full level");
                expect (Rig::rms (r.mL) > unity * 0.98 && Rig::rms (r.mL) < unity * 1.02, "master carries only ch1");

                r.mixer.setPfl (0, false);
                r.run();
                expect (Rig::rms (r.cL) < 1e-6, "cue is silent without PFL");
            }

            beginTest ("Cue mix blends master into headphones, cue level scales");
            {
                Rig r;
                r.setSine (0, 1000.0); r.setSine (1, 3000.0);
                r.mixer.setPfl (0, true);
                r.mixer.setFader (0, 0.0f);   // ch0 only in cue
                r.mixer.setCueMix (1.0f);
                r.run();
                const double masterRms = Rig::rms (r.mL);
                expectWithinAbsoluteError (Rig::rms (r.cL), masterRms, 1e-4);
                r.mixer.setCueMix (0.5f);
                r.mixer.setCueLevel (0.5f);
                r.run();
                expect (Rig::rms (r.cL) > 0.0);
                expect (Rig::rms (r.cL) < masterRms, "scaled below master");
            }

            beginTest ("Meters track level and decay");
            {
                Rig r;
                r.setSine (0, 1000.0, 0.5f);
                r.run();
                expect (r.mixer.getChannelPeak (0) > 0.1f);
                expect (r.mixer.getMasterPeakLeft() > 0.1f);
                expectWithinAbsoluteError (r.mixer.getChannelPeak (1), 0.0f, 1e-6f);
                r.setSine (0, 1000.0, 0.0f);
                for (int i = 0; i < 6; ++i) r.run();
                expect (r.mixer.getChannelPeak (0) < 0.01f, "peak decays after silence");
            }

            beginTest ("Null inputs produce silence; oversize blocks are chunked");
            {
                Rig r;
                Mixer::ChannelInput none[2];
                r.mixer.process (none, r.mL.data(), r.mR.data(), r.cL.data(), r.cR.data(), 512);
                for (int i = 0; i < 512; ++i) expectEquals (r.mL[(size_t) i], 0.0f);

                r.setSine (0, 1000.0);
                Mixer::ChannelInput ci[2] = { { r.in[0][0].data(), r.in[0][1].data() }, {} };
                r.mixer.process (ci, r.mL.data(), r.mR.data(), r.cL.data(), r.cR.data(), N);   // N > prepared 512
                expectWithinAbsoluteError (Rig::rms (r.mL), 0.5 / std::sqrt (2.0), 0.005);
            }
        }
    };

    MixerTest mixerTest;
}
