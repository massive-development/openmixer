#include <JuceHeader.h>
#include "engine/DjEngine.h"
#include "engine/MockAudioDevice.h"

#include <cmath>

namespace OpenMixerTests
{
    class DjEngineTest : public juce::UnitTest
    {
    public:
        DjEngineTest() : juce::UnitTest ("DJ Engine", "OpenMixer Engine") {}

        static constexpr int kFrames = 44100;
        static constexpr double kRate = 44100.0;

        static juce::File writeSine (const juce::File& f)
        {
            f.deleteFile();
            juce::WavAudioFormat wav;
            std::unique_ptr<juce::FileOutputStream> stream (f.createOutputStream());
            std::unique_ptr<juce::AudioFormatWriter> writer (wav.createWriterFor (stream.get(), kRate, 2, 24, {}, 0));
            stream.release();   // writer owns the stream

            juce::AudioBuffer<float> buf (2, kFrames);
            for (int i = 0; i < kFrames; ++i)
                for (int c = 0; c < 2; ++c)
                    buf.setSample (c, i, 0.5f * std::sin (2.0f * juce::MathConstants<float>::pi * 440.0f * (float) i / (float) kRate));
            writer->writeFromAudioSampleBuffer (buf, 0, kFrames);
            return f;
        }

        static bool pumpUntil (openmixer::MockAudioDevice& dev, const std::function<bool()>& cond)
        {
            const auto end = juce::Time::getMillisecondCounter() + 3000;
            while (! cond())
            {
                if (juce::Time::getMillisecondCounter() > end) return false;
                dev.renderBlock();
                juce::Thread::sleep (1);
            }
            return true;
        }

        void runTest() override
        {
            using namespace openmixer;
            auto dir = juce::File::getSpecialLocation (juce::File::tempDirectory).getChildFile ("openmixer_engine_test");
            dir.createDirectory();
            const auto file = writeSine (dir.getChildFile ("sine.wav"));

            beginTest ("Silent when nothing is loaded");
            {
                DjEngine engine;
                MockAudioDevice dev (kRate, 256, 0, 4);
                dev.start (engine);
                dev.renderBlocks (4);
                for (int c = 0; c < 4; ++c)
                    expectEquals (dev.getLastOutput().getMagnitude (c, 0, 256), 0.0f);
            }

            beginTest ("Deck audio reaches master; PFL reaches cue; fader gates master");
            {
                DjEngine engine;
                MockAudioDevice dev (kRate, 256, 0, 4);
                expect (engine.getDeck (0).load (file));
                dev.start (engine);
                engine.getMixer().setPfl (0, true);
                engine.getMixer().setFader (0, 0.0f);
                engine.getDeck (0).play();

                expect (pumpUntil (dev, [&] { return engine.getDeck (0).getPositionFrames() > 2000; }));
                expect (dev.getLastOutput().getMagnitude (0, 0, 256) < 1e-6f, "fader at zero silences master");
                expect (dev.getLastOutput().getMagnitude (2, 0, 256) > 0.1f, "PFL carries pre-fader audio");

                engine.getMixer().setFader (0, 1.0f);
                dev.renderBlocks (4);
                expect (dev.getLastOutput().getMagnitude (0, 0, 256) > 0.1f);
                expect (engine.getMixer().getChannelPeak (0) > 0.1f);
            }

            beginTest ("Works on a 2-channel and a 1-channel device");
            {
                for (int outs : { 1, 2 })
                {
                    DjEngine engine;
                    MockAudioDevice dev (kRate, 256, 0, outs);
                    engine.getDeck (1).load (file);
                    dev.start (engine);
                    engine.getDeck (1).play();
                    expect (pumpUntil (dev, [&] { return engine.getDeck (1).getPositionFrames() > 2000; }));
                    dev.renderBlocks (2);
                    expect (dev.getLastOutput().getMagnitude (0, 0, 256) > 0.1f, "outs=" + juce::String (outs));
                }
            }

            beginTest ("Oversize blocks are chunked");
            {
                DjEngine engine;
                engine.prepare (kRate, 64);
                juce::AudioBuffer<float> out (2, 300);
                float* ptrs[2] { out.getWritePointer (0), out.getWritePointer (1) };
                engine.process (nullptr, 0, ptrs, 2, 300);
                expectEquals (out.getMagnitude (0, 300), 0.0f);
            }

            dir.deleteRecursively();
        }
    };

    DjEngineTest djEngineTest;
}
