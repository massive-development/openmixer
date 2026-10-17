// Deck: transport plus streaming decode. See docs/adr for the seek/load epoch handshake.
// Loops wrap on the audio thread into a decoder-prefetched slot, so loop jumps are gapless.
#pragma once

#include "../core/ParameterTarget.h"
#include "AudioDevice.h"
#include "HotcueBank.h"
#include "LoopPrefetch.h"
#include "LoopState.h"
#include "Quantizer.h"
#include "RingBuffer.h"
#include "TimeStretch.h"

#include <juce_audio_formats/juce_audio_formats.h>

#include <atomic>
#include <memory>
#include <mutex>
#include <optional>
#include <vector>

namespace openmixer
{

class DeckDecoder;

class Deck : public AudioCallback, public ParameterTarget
{
public:
    explicit Deck (size_t ringFrames = 1U << 16, juce::int64 loopPrefetchFrames = 1 << 16);
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
    // Key-lock: tempo follows rate, pitch stays put (Rubber Band). Needs prepare() first.
    void setKeylock (bool enabled) noexcept { keylock.store (enabled, std::memory_order_relaxed); }
    bool isKeylock() const noexcept { return keylock.load (std::memory_order_relaxed); }

    // Loops: bounds in source frames (quantize them with Quantizer first). Control threads.
    // An active loop captures the playhead once it is past the loop start; seeking outside exits it.
    bool setLoop (const Loop& loop) noexcept;
    void exitLoop() noexcept   { setLoopEnabled (false); }
    void reloop() noexcept     { setLoopEnabled (true); }
    bool isLooping() const noexcept;
    Loop getLoop() const noexcept;
    bool isLoopPrefetched() const noexcept { return prefetch.isPublished(); }   // a loop jump will be gapless

    // Hotcues: trigger sets an empty cue at the playhead, otherwise jumps to it.
    void triggerHotcue (int index);
    void clearHotcue (int index) noexcept   { hotcues.clear (index); }
    std::optional<juce::int64> getHotcue (int index) const noexcept { return hotcues.get (index); }
    void setHotcue (int index, juce::int64 frame) noexcept { hotcues.set (index, frame); }

    // Ids: play (>= 0.5 plays), rate, keylock, seek (0..1 of length), loop.exit, loop.reloop, hotcueN, hotcueN.clear
    bool setParameter (const juce::String& id, double value) override;

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
    void setLoopEnabled (bool enabled) noexcept;
    bool setHotcueParameter (const juce::String& id, double value);

    // Audio thread (DeckPlayback.cpp)
    bool serviceHandshake() noexcept;
    void resetPlayhead (juce::uint32 ack) noexcept;
    bool primeInterpolator() noexcept;
    bool popFrame() noexcept;
    bool pullFrame (float* out, juce::int64& index) noexcept;
    void enterLoopSlot() noexcept;
    bool readSlotFrame (float* out) noexcept;
    bool readRingFrame (float* out) noexcept;
    void releaseSlot() noexcept;
    void requestRing (juce::int64 target) noexcept;
    bool reachedEnd() noexcept;
    void noteStarved() noexcept;
    void clearTail (float* left, float* right, int from, int numSamples) const noexcept;
    void renderInterpolated (float* left, float* right, int numSamples) noexcept;

    // Audio thread, key-lock path (DeckKeylock.cpp)
    void renderKeylocked (float* left, float* right, int numSamples) noexcept;
    void enterKeylock() noexcept;
    void leaveKeylock() noexcept;
    bool feedStretch() noexcept;
    int drainStretch (float* left, float* right, int wanted) noexcept;

    SpscRingBuffer<float> ring;   // interleaved L,R
    LoopPrefetch prefetch;
    LoopState loopState;
    HotcueBank hotcues;

    std::atomic<bool> playing { false }, loaded { false }, finished { false }, keylock { false };
    std::atomic<double> rate { 1.0 };
    std::atomic<double> deviceSampleRate { 44100.0 };
    std::atomic<double> fileSampleRate { 44100.0 };
    std::atomic<juce::int64> lengthFrames { 0 };
    std::atomic<juce::int64> positionFrames { 0 };
    std::atomic<juce::uint32> underruns { 0 };

    // Control seeks: requestEpoch -> ackEpoch. Every ring reposition (control or audio) bumps ackSeq.
    std::atomic<juce::uint32> requestEpoch { 0 }, ackEpoch { 0 }, ackSeq { 0 }, drainedSeq { 0 };
    std::atomic<juce::int64> seekTarget { 0 }, ackTarget { 0 };
    std::atomic<juce::uint32> ackLoadId { 0 };
    std::atomic<bool> endOfStream { false };

    // Audio-thread ring repositions (after a loop jump); based on a control epoch so stale ones are dropped.
    std::atomic<juce::uint32> ringReqEpoch { 0 }, ringReqBase { 0 };
    std::atomic<juce::int64> ringReqTarget { 0 };

    std::mutex pendingMutex;   // control -> decoder reader hand-over only
    std::unique_ptr<juce::AudioFormatReader> pendingReader;
    bool hasPendingReader = false;
    juce::uint32 pendingLoadId = 0, loadCounter = 0;
    juce::AudioFormatManager formatManager;

    // audio-thread-only state
    bool primed = false, haveFirst = false;
    double phase = 0.0;
    float prev[2] { 0.0F, 0.0F }, next[2] { 0.0F, 0.0F };
    juce::int64 prevIdx = 0, nextIdx = 0;   // source frames held by the interpolator
    juce::int64 readIdx = 0;                // next source frame to pull
    juce::int64 ringIdx = 0;                // source frame at the ring's read head
    juce::int64 ringWanted = 0;
    juce::uint32 lastControlAck = 0, playingLoadId = 0;
    const LoopPrefetch::Slot* slot = nullptr;
    LoopSnapshot loopSnap;
    std::vector<float> monoSink;
    TimeStretch stretch;
    std::vector<float> stretchIn[2];
    bool stretchActive = false, feedNextFirst = false;
    int stretchDiscard = 0;
    double stretchRatio = 1.0;
    juce::int64 lastFedIdx = 0;

    std::unique_ptr<DeckDecoder> decoder;
};

} // namespace openmixer
