#include <JuceHeader.h>
#include "engine/Deck.h"

namespace OpenMixerTests
{
    class DeckTest : public juce::UnitTest
    {
    public:
        DeckTest() : juce::UnitTest ("Deck", "OpenMixer Engine") {}

        static constexpr int kFrames = 20000;
        static constexpr double kRate = 44100.0;

        /** Writes a stereo WAV: left = n / kFrames, right = -left. */
        static juce::File writeRamp (const juce::File& f, int numChannels)
        {
            f.deleteFile();
            juce::WavAudioFormat wav;
            std::unique_ptr<juce::FileOutputStream> stream (f.createOutputStream());
            std::unique_ptr<juce::AudioFormatWriter> writer (
                wav.createWriterFor (stream.get(), kRate, (unsigned) numChannels, 24, {}, 0));
            stream.release();   // writer owns it now

            juce::AudioBuffer<float> buf (numChannels, kFrames);
            for (int i = 0; i < kFrames; ++i)
                for (int c = 0; c < numChannels; ++c)
                    buf.setSample (c, i, (c == 0 ? 1.0f : -1.0f) * (float) i / (float) kFrames);
            writer->writeFromAudioSampleBuffer (buf, 0, kFrames);
            return f;
        }

        static bool waitFor (const std::function<bool()>& cond, int timeoutMs = 3000)
        {
            const auto end = juce::Time::getMillisecondCounter() + (juce::uint32) timeoutMs;
            while (! cond())
            {
                if (juce::Time::getMillisecondCounter() > end) return false;
                juce::Thread::sleep (2);
            }
            return true;
        }

        /** Pump blocks until the deck has produced non-silent output or timeout. */
        static void pumpUntilBuffered (openmixer::Deck& deck, size_t frames, juce::int64 expectedPos = 0)
        {
            float l[64], r[64];
            waitFor ([&]
            {
                deck.renderStereo (l, r, 64);   // services the seek handshake while paused
                return deck.getPositionFrames() == expectedPos && deck.getBufferedFrames() >= frames;
            });
        }

        void runTest() override
        {
            auto dir = juce::File::getSpecialLocation (juce::File::tempDirectory).getChildFile ("openmixer_deck_test");
            dir.createDirectory();
            auto wavFile = writeRamp (dir.getChildFile ("ramp.wav"), 2);

            beginTest ("Load rejects non-audio files");
            {
                openmixer::Deck deck;
                auto junk = dir.getChildFile ("junk.wav");
                junk.replaceWithText ("not audio");
                expect (! deck.load (junk));
                expect (! deck.isLoaded());
            }

            beginTest ("Load reports length and sample rate");
            {
                openmixer::Deck deck;
                expect (deck.load (wavFile));
                expect (deck.isLoaded());
                expectEquals ((int) deck.getLengthFrames(), kFrames);
                expectEquals (deck.getFileSampleRate(), kRate);
            }

            beginTest ("Paused deck outputs silence and does not advance");
            {
                openmixer::Deck deck;
                deck.prepare (kRate, 256);
                deck.load (wavFile);
                pumpUntilBuffered (deck, 1000);
                std::vector<float> l (256, 1.0f), r (256, 1.0f);
                deck.renderStereo (l.data(), r.data(), 256);
                for (auto v : l) expectEquals (v, 0.0f);
                expectEquals ((int) deck.getPositionFrames(), 0);
            }

            beginTest ("Playback at rate 1.0 reproduces the source and finishes");
            {
                openmixer::Deck deck (1 << 12);   // small ring forces many wraparounds
                deck.prepare (kRate, 256);
                deck.load (wavFile);
                pumpUntilBuffered (deck, 512);
                deck.play();

                std::vector<float> out;
                std::vector<float> l (256), r (256);
                bool ok = waitFor ([&]
                {
                    // Pace like a device: only render when a full block is buffered
                    // (or everything remaining is already buffered).
                    if (deck.getBufferedFrames() < 300
                        && deck.getPositionFrames() + (juce::int64) deck.getBufferedFrames() < kFrames - 3)
                        return false;
                    deck.renderStereo (l.data(), r.data(), 256);
                    out.insert (out.end(), l.begin(), l.end());
                    return deck.hasFinished();
                }, 5000);
                expect (ok, "deck should reach the end");
                expect (! deck.isPlaying());

                // Starting from frame 0, output frame i == source frame i until the
                // final frame (which is the interpolation anchor and never emitted).
                bool match = out.size() >= (size_t) kFrames - 1;
                for (int i = 0; match && i < kFrames - 1; ++i)
                    if (std::abs (out[(size_t) i] - (float) i / (float) kFrames) > 1e-4f)
                    {
                        match = false;
                        expect (false, "mismatch at frame " + juce::String (i));
                    }
                expect (match);
            }

            beginTest ("Underrun is counted and outputs silence when decoder is starved");
            {
                openmixer::Deck deck (1 << 12);
                deck.prepare (kRate, 256);
                deck.load (wavFile);
                pumpUntilBuffered (deck, 2048);
                deck.setRate (8.0);   // consume far faster than the ring can hold
                deck.play();
                std::vector<float> l (4096, 1.0f), r (4096, 1.0f);
                deck.renderStereo (l.data(), r.data(), 4096);
                expect (deck.getUnderrunCount() >= 1, "underrun should be counted");
                expectEquals (l[4095], 0.0f);   // tail is silence
                expect (l[1] != 0.0f);          // head had real audio
                expect (deck.isPlaying(), "an underrun is not end-of-file");
            }

            beginTest ("Seek repositions playback");
            {
                openmixer::Deck deck;
                deck.prepare (kRate, 256);
                deck.load (wavFile);
                pumpUntilBuffered (deck, 512);
                deck.seek (10000);
                std::vector<float> l (64), r (64);
                bool synced = waitFor ([&]
                {
                    deck.renderStereo (l.data(), r.data(), 64);
                    return deck.getPositionFrames() == 10000 && deck.getBufferedFrames() > 256;
                });
                expect (synced, "seek handshake should complete");
                deck.play();
                waitFor ([&] { deck.renderStereo (l.data(), r.data(), 64); return l[0] != 0.0f; });
                expectWithinAbsoluteError (l[0], 10000.0f / (float) kFrames, 1e-4f);
                expectWithinAbsoluteError (r[0], -10000.0f / (float) kFrames, 1e-4f);
                expectWithinAbsoluteError (l[1] - l[0], 1.0f / (float) kFrames, 1e-6f);
            }

            beginTest ("Rate 0.5 advances half as fast and interpolates");
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
                expect (std::abs (deck.getPositionFrames() - (4000 + 500)) <= 2,
                        "position " + juce::String (deck.getPositionFrames()));
                // Linear ramp => interpolation is exact; slope is half.
                expectWithinAbsoluteError (l[2] - l[0], 1.0f / (float) kFrames, 1e-6f);
            }

            beginTest ("Mono files play on both channels");
            {
                auto mono = writeRamp (dir.getChildFile ("mono.wav"), 1);
                openmixer::Deck deck;
                deck.prepare (kRate, 256);
                deck.load (mono);
                pumpUntilBuffered (deck, 512);
                deck.seek (5000);
                std::vector<float> l (64), r (64);
                waitFor ([&] { deck.renderStereo (l.data(), r.data(), 64); return deck.getPositionFrames() == 5000 && deck.getBufferedFrames() > 128; });
                deck.play();
                deck.renderStereo (l.data(), r.data(), 64);
                expectWithinAbsoluteError (l[0], r[0], 1e-7f);
                expect (l[0] > 0.2f);
            }

            beginTest ("Deck runs under a mock audio device");
            {
                openmixer::Deck deck;
                openmixer::MockAudioDevice dev (kRate, 256, 0, 2);
                deck.load (wavFile);
                dev.start (deck);
                dev.renderBlocks (1);
                waitFor ([&] { dev.renderBlock(); return deck.getBufferedFrames() > 1024; });
                deck.play();
                waitFor ([&] { dev.renderBlock(); return deck.getPositionFrames() > 1000; });
                expect (deck.getPositionFrames() > 1000);
                expect (dev.getLatencyStats().callbackCount > 0);
                dev.stop();
            }

            dir.deleteRecursively();
        }
    };

    DeckTest deckTest;
}
