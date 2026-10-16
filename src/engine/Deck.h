/*
  OpenMixer - Deck engine: transport + streaming decode.

  A background decoder thread reads a file through juce::AudioFormatManager
  and pushes interleaved stereo frames into a lock-free SPSC ring buffer. The
  audio thread (Deck::process) pops from the ring with linear-interpolation
  varispeed. Nothing on the audio thread allocates or locks.

  Seeks and loads never reset the ring from the producer side. Instead they
  use an epoch handshake:
    1. control thread publishes a target and bumps `requestEpoch`
    2. decoder repositions, then publishes `ackEpoch` (and writes nothing more)
    3. audio thread sees ack == request, drains the stale ring, publishes
       `drainedEpoch`
    4. decoder resumes writing fresh data
  Output is silent while a handshake is pending.

  Follow-up: an FFmpeg decode backend (see TODO Wave 2) can replace the
  AudioFormatManager reader behind the same interface.
*/
#pragma once

#include "AudioDevice.h"
#include "RingBuffer.h"

#include <juce_audio_formats/juce_audio_formats.h>

#include <atomic>
#include <memory>
#include <mutex>

namespace openmixer
{

class Deck : public AudioCallback
{
public:
    /** ringFrames: ring capacity in stereo frames (rounded up to a power of two). */
    explicit Deck (size_t ringFrames = 1 << 16);
    ~Deck() override;

    // Control thread API ----------------------------------------------------
    /** Loads a file asynchronously; returns false if no format can read it. */
    bool load (const juce::File& file);
    void unload();

    void play() noexcept          { playing.store (true); }
    void pause() noexcept         { playing.store (false); }
    void setPlaying (bool shouldPlay) noexcept { playing.store (shouldPlay); }
    /** Seek to a source frame; asynchronous via the epoch handshake. */
    void seek (juce::int64 sourceFrame);
    /** Playback rate multiplier (1.0 = normal). Varispeed: pitch follows rate. */
    void setRate (double newRate) noexcept;

    // Status (any thread) ---------------------------------------------------
    bool isLoaded() const noexcept        { return loaded.load(); }
    bool isPlaying() const noexcept       { return playing.load(); }
    double getRate() const noexcept       { return rate.load(); }
    juce::int64 getPositionFrames() const noexcept { return positionFrames.load(); }
    juce::int64 getLengthFrames() const noexcept   { return lengthFrames.load(); }
    double getFileSampleRate() const noexcept      { return fileSampleRate.load(); }
    bool hasFinished() const noexcept     { return finished.load(); }
    juce::uint32 getUnderrunCount() const noexcept { return underruns.load(); }
    size_t getBufferedFrames() const noexcept      { return ring.getReadableCount() / 2; }

    // AudioCallback ---------------------------------------------------------
    void prepare (double sampleRate, int maxBlockSize) override;
    void process (const float* const* inputs, int numInputs,
                  float* const* outputs, int numOutputs, int numSamples) override;

    /** Audio-thread render of one stereo block (left/right overwritten). */
    void renderStereo (float* left, float* right, int numSamples) noexcept;

private:
    class DecoderThread;

    void requestReposition (juce::int64 target);

    SpscRingBuffer<float> ring;   // interleaved L,R

    // control -> audio/decoder
    std::atomic<bool> playing { false }, loaded { false }, finished { false };
    std::atomic<double> rate { 1.0 };
    std::atomic<double> deviceSampleRate { 44100.0 };
    std::atomic<double> fileSampleRate { 44100.0 };
    std::atomic<juce::int64> lengthFrames { 0 };
    std::atomic<juce::int64> positionFrames { 0 };
    std::atomic<juce::uint32> underruns { 0 };

    // epoch handshake
    std::atomic<juce::uint32> requestEpoch { 0 }, ackEpoch { 0 }, drainedEpoch { 0 };
    std::atomic<juce::int64> seekTarget { 0 }, ackTarget { 0 };
    std::atomic<bool> endOfStream { false };

    // reader hand-over (control thread -> decoder thread; never touched by audio thread)
    std::mutex pendingMutex;
    std::unique_ptr<juce::AudioFormatReader> pendingReader;
    bool hasPendingReader = false;
    juce::AudioFormatManager formatManager;

    // audio-thread-only state
    bool primed = false;
    double phase = 0.0;
    float prev[2] { 0, 0 }, next[2] { 0, 0 };
    juce::int64 pos = 0;

    std::unique_ptr<DecoderThread> decoder;
};

} // namespace openmixer
