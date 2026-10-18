// OpenMixer ControlThreadSafetyTest - MIDI thread vs UI thread: router (un)registration and map reloads while messages stream
#include <JuceHeader.h>
#include "controllers/MidiMapper.h"
#include "controllers/ParameterRouter.h"
#include "controllers/RecordingTarget.h"

#include <gtest/gtest.h>

#include <atomic>
#include <memory>
#include <thread>

using namespace openmixer;

namespace
{
// Counts calls and records any that arrive after the owner declared it gone.
struct Probe final : ParameterTarget
{
    bool setParameter (const juce::String&, double) override
    {
        if (gone.load())
            ++afterGone;
        ++calls;
        return true;
    }
    std::atomic<bool> gone { false };
    std::atomic<int> afterGone { 0 }, calls { 0 };
};

constexpr int kRounds = 300;
}

TEST (ControlThreadSafetyTest, RouterSetParameterRacesWithRegisterAndUnregister)
{
    ParameterRouter router;
    Probe stable;
    router.registerTarget ("stable", &stable);
    std::atomic<bool> stop { false };
    std::atomic<int> handled { 0 };
    std::thread midi ([&]
    {
        while (! stop.load())
        {
            router.setParameter ("stable.x", 1.0);
            if (router.setParameter ("plugin.x", 0.5))
                ++handled;
            std::this_thread::sleep_for (std::chrono::microseconds (20));   // a hot loop on an unfair mutex starves the writer (seen under valgrind)
        }
    });

    int afterGone = 0;
    for (int i = 0; i < kRounds; ++i)
    {
        auto target = std::make_unique<Probe>();   // freed after unregister: a late call would be a use-after-free
        router.registerTarget ("plugin", target.get());
        std::this_thread::sleep_for (std::chrono::microseconds (200));
        router.unregisterTarget ("plugin");
        target->gone = true;
        std::this_thread::sleep_for (std::chrono::microseconds (50));
        afterGone += target->afterGone.load();
        target.reset();
    }
    stop = true;
    midi.join();
    EXPECT_EQ (afterGone, 0) << "no call may reach a target once unregister returned";
    EXPECT_GT (stable.calls.load(), 0);
}

TEST (ControlThreadSafetyTest, UnregisterWaitsForAnInFlightCall)
{
    struct Slow final : ParameterTarget
    {
        bool setParameter (const juce::String&, double) override
        {
            entered = true;
            std::this_thread::sleep_for (std::chrono::milliseconds (100));
            finished = true;
            return true;
        }
        std::atomic<bool> entered { false }, finished { false };
    } slow;
    ParameterRouter router;
    router.registerTarget ("slow", &slow);
    std::thread caller ([&] { router.setParameter ("slow.x", 1.0); });
    while (! slow.entered.load())
        std::this_thread::yield();
    router.unregisterTarget ("slow");
    EXPECT_TRUE (slow.finished.load()) << "unregister returned while the call was still running";
    caller.join();
}

TEST (ControlThreadSafetyTest, MapReloadWhileMessagesStream)
{
    const char* mapA = R"({"mappings":[{"type":"cc","number":7,"target":"a","min":0,"max":1}]})";
    const char* mapB = R"({"mappings":[{"type":"cc","number":7,"target":"b","min":0,"max":1},{"type":"cc","number":8,"target":"c"}]})";
    MidiMapper mapper;
    RecordingTarget target;
    mapper.setTarget (&target);
    ASSERT_TRUE (mapper.loadFromJson (mapA).wasOk());

    std::atomic<bool> stop { false };
    std::atomic<int> fired { 0 };
    std::thread midi ([&]
    {
        int v = 0;
        while (! stop.load())
        {
            fired += mapper.handleMessage (juce::MidiMessage::controllerEvent (1, 7, v++ & 127));
            fired += mapper.handleMessage (juce::MidiMessage::controllerEvent (1, 8, 1));
            mapper.getValue ("a");
            std::this_thread::sleep_for (std::chrono::microseconds (20));
        }
    });
    const auto deadline = juce::Time::getMillisecondCounter() + 20000;
    while (fired.load() == 0 && juce::Time::getMillisecondCounter() < deadline)   // let the stream start (slow under valgrind)
        std::this_thread::sleep_for (std::chrono::milliseconds (1));
    for (int i = 0; i < kRounds; ++i)
        ASSERT_TRUE (mapper.loadFromJson (i % 2 == 0 ? mapB : mapA).wasOk());
    EXPECT_TRUE (mapper.loadFromJson ("not json").failed());   // a bad reload leaves the last good map
    stop = true;
    midi.join();
    EXPECT_GT (fired.load(), 0);
    EXPECT_GE (static_cast<int> (mapper.getNumMappings()), 1);
}
