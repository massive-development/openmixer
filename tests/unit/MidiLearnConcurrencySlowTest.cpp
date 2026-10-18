// OpenMixer MidiLearnConcurrencySlowTest - MIDI thread streams while the message thread arms, cancels and edits the map
#include <JuceHeader.h>
#include "controllers/MidiMapper.h"
#include "controllers/RecordingTarget.h"

#include <gtest/gtest.h>

#include <atomic>
#include <thread>

using namespace openmixer;

namespace
{
struct CountingTarget final : ParameterTarget
{
    bool setParameter (const juce::String&, double) override { ++calls; return true; }
    std::atomic<int> calls { 0 };
};
}

TEST (MidiLearnConcurrencySlowTest, ArmCancelAndEditWhileMessagesStream)
{
    MidiMapper mapper;
    CountingTarget target;
    mapper.setTarget (&target);
    std::atomic<int> learned { 0 };
    mapper.setOnLearned ([&] (const Mapping&) { ++learned; });
    Mapping seed;
    seed.number = 1; seed.target = "seed";
    ASSERT_TRUE (mapper.addBinding (seed).wasOk());

    std::atomic<bool> stop { false };
    std::thread midi ([&]
    {
        int i = 0;
        while (! stop.load())
        {
            mapper.handleMessage (juce::MidiMessage::controllerEvent (1, 1 + (i % 4), i % 128));
            mapper.handleMessage (juce::MidiMessage::noteOn (1, 40 + (i % 8), static_cast<juce::uint8> (100)));
            ++i;
        }
    });

    LearnRequest r;
    r.target = "learned";
    for (int round = 0; round < 400; ++round)
    {
        mapper.armLearn (r);
        if (round % 3 == 0)
            mapper.cancelLearn();
        Mapping extra;
        extra.number = 20 + (round % 5);
        extra.target = "extra" + juce::String (round % 5);
        mapper.addBinding (extra);
        if (round % 4 == 0)
            mapper.removeBindingsFor (extra.target);
        mapper.getBindings();
        mapper.setOutputs ({});
        if (round % 50 == 0)
            std::this_thread::sleep_for (std::chrono::milliseconds (1));
    }
    stop = true;
    midi.join();
    EXPECT_GT (learned.load(), 0);
    EXPECT_GT (target.calls.load(), 0);
    EXPECT_FALSE (mapper.getBindings().empty());
}
