// Shared helpers for Deck tests: ramp WAV fixture, bounded polling.
#pragma once

#include <JuceHeader.h>
#include "engine/Deck.h"

#include <gtest/gtest.h>

#include <functional>
#include <thread>

namespace deck_test
{

inline constexpr int kFrames = 20000;
inline constexpr double kRate = 44100.0;

/** Writes a WAV: left = n / kFrames, right = -left. */
inline juce::File writeRamp (const juce::File& f, int numChannels)
{
    f.deleteFile();
    juce::WavAudioFormat wav;
    std::unique_ptr<juce::FileOutputStream> stream (f.createOutputStream());
    std::unique_ptr<juce::AudioFormatWriter> writer (
        wav.createWriterFor (stream.get(), kRate, static_cast<unsigned> (numChannels), 24, {}, 0));
    stream.release();   // writer owns it now

    juce::AudioBuffer<float> buf (numChannels, kFrames);
    for (int i = 0; i < kFrames; ++i)
        for (int c = 0; c < numChannels; ++c)
            buf.setSample (c, i, (c == 0 ? 1.0f : -1.0f) * static_cast<float> (i) / static_cast<float> (kFrames));
    writer->writeFromAudioSampleBuffer (buf, 0, kFrames);
    return f;
}

/** Bounded poll: the deck's decoder thread exposes no signal to wait on. */
inline bool waitFor (const std::function<bool()>& cond, int timeoutMs = 3000)
{
    const auto end = juce::Time::getMillisecondCounter() + static_cast<juce::uint32> (timeoutMs);
    while (! cond())
    {
        if (juce::Time::getMillisecondCounter() > end) return false;
        std::this_thread::yield();
    }
    return true;
}

/** Pump blocks until the deck is at expectedPos with enough buffered, or timeout. */
inline bool pumpUntilBuffered (openmixer::Deck& deck, size_t frames, juce::int64 expectedPos = 0)
{
    float l[64], r[64];
    return waitFor ([&]
    {
        deck.renderStereo (l, r, 64);   // services the seek handshake while paused
        return deck.getPositionFrames() == expectedPos && deck.getBufferedFrames() >= frames;
    });
}

class DeckFixture : public ::testing::Test
{
protected:
    void SetUp() override
    {
        dir = juce::File::getSpecialLocation (juce::File::tempDirectory)
                  .getChildFile ("openmixer_deck_test_" + juce::Uuid().toString());   // unique per test process: ctest -j runs them in parallel
        ASSERT_TRUE (dir.createDirectory());
        wavFile = writeRamp (dir.getChildFile ("ramp.wav"), 2);
    }

    void TearDown() override { dir.deleteRecursively(); }

    juce::File dir, wavFile;
};
}
