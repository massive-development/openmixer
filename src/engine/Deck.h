// Deck: transport plus streaming decode. See docs/adr for the seek/load epoch handshake.
#pragma once

#include "AudioDevice.h"
#include "RingBuffer.h"

#include <juce_audio_formats/juce_audio_formats.h>

#include <atomic>
#include <memory>
#include <mutex>
#include <vector>

namespace openmixer
{

class DeckDecoder;

class Deck : public AudioCallback
{
public:
    explicit Deck (size_t ringFrames = 1U << 16);
    ~Deck() override;

    Deck (const Deck&) = delete;
    Deck& operator= (const Deck&) = delete;

    bool load (const juce::File& file);
    void unload();

    void play() noexcept          { setPlaying (true); }
    void pause() noexcept         { setPlaying (false); }
    void setPlaying (bool shouldPlay) noexcept { playing.store (shouldPlay, std::memory_order_relaxed); }
    void seek (juce::int64 sourceFrame);
    void setRate (double newRate) noexcept;

    bool isLoaded() const noexcept        { return loaded.load (std::memory_order_relaxed); }
    bool isPlaying() const noexcept       { return playing.load (std::memory_order_relaxed); }
    double getRate() const noexcept       { return rate.load (std::memory_order_relaxed); }
    juce::int64 getPositionFrames() const noexcept { return positionFrames.load (std::memory_order_relaxed); }
    juce::int64 getLengthFrames() const noexcept   { return lengthFrames.load (std::memory_order_relaxed); }
    double getFileSampleRate() const noexcept      { return fileSampleRate.load (std::memory_order_relaxed); }
    bool hasFinished() const noexcept     { return finished.load (std::memory_order_relaxed); }
    juce::uint32 getUnderrunCount() const noexcept { return underruns.load (std::memory_order_relaxed); }
    size_t getBufferedFrames() const noexcept      { return ring.getReadableCount() / 2; }

    void prepare (double sampleRate, int maxBlockSize) override;
    void process (const float* const* inputs, int numInputs,
                  float* const* outputs, int numOutputs, int numSamples) override;

    void renderStereo (float* left, float* right, int numSamples) noexcept;

private:
    friend class DeckDecoder;

    void requestReposition (juce::int64 target);
    bool serviceHandshake() noexcept;
    bool primeInterpolator() noexcept;
    bool popFrame() noexcept;
    bool reachedEnd() noexcept;
    void noteStarved() noexcept;
    void clearTail (float* left, float* right, int from, int numSamples) const noexcept;

    SpscRingBuffer<float> ring;   // interleaved L,R

    std::atomic<bool> playing { false }, loaded { false }, finished { false };
    std::atomic<double> rate { 1.0 };
    std::atomic<double> deviceSampleRate { 44100.0 };
    std::atomic<double> fileSampleRate { 44100.0 };
    std::atomic<juce::int64> lengthFrames { 0 };
    std::atomic<juce::int64> positionFrames { 0 };
    std::atomic<juce::uint32> underruns { 0 };

    std::atomic<juce::uint32> requestEpoch { 0 }, ackEpoch { 0 }, drainedEpoch { 0 };
    std::atomic<juce::int64> seekTarget { 0 }, ackTarget { 0 };
    std::atomic<bool> endOfStream { false };

    std::mutex pendingMutex;   // control -> decoder reader hand-over only
    std::unique_ptr<juce::AudioFormatReader> pendingReader;
    bool hasPendingReader = false;
    juce::AudioFormatManager formatManager;

    // audio-thread-only state
    bool primed = false;
    double phase = 0.0;
    float prev[2] { 0.0F, 0.0F }, next[2] { 0.0F, 0.0F };
    juce::int64 pos = 0;
    std::vector<float> monoSink;

    std::unique_ptr<DeckDecoder> decoder;
};

} // namespace openmixer
