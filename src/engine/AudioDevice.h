/*
  OpenMixer - audio device layer.

  AudioDevice is a small abstraction over a real-time audio backend. The JUCE
  implementation wraps juce::AudioDeviceManager (platform backends such as
  ALSA/CoreAudio/WASAPI are left to JUCE); MockAudioDevice runs headless for
  CI and tests. Both feed a LatencyMonitor so callback timing and device
  latency are observable.

  Real-time rules: AudioCallback::process and LatencyMonitor::recordCallback
  never allocate, lock or block.
*/
#pragma once

#include <juce_audio_basics/juce_audio_basics.h>
#include <juce_audio_devices/juce_audio_devices.h>

#include <atomic>
#include <memory>
#include <vector>

namespace openmixer
{

/** Receives audio from a device. process() runs on the audio thread. */
class AudioCallback
{
public:
    virtual ~AudioCallback() = default;
    virtual void prepare (double /*sampleRate*/, int /*maxBlockSize*/) {}
    virtual void process (const float* const* inputs, int numInputs,
                          float* const* outputs, int numOutputs, int numSamples) = 0;
    virtual void release() {}
};

struct LatencyStats
{
    double sampleRate = 0.0;
    int bufferSize = 0;
    int inputLatencySamples = 0;
    int outputLatencySamples = 0;
    double inputLatencyMs = 0.0;
    double outputLatencyMs = 0.0;
    double roundTripLatencyMs = 0.0;   // buffer + input + output latency
    uint64_t callbackCount = 0;
    double lastCallbackUs = 0.0;
    double avgCallbackUs = 0.0;
    double maxCallbackUs = 0.0;
    double budgetUs = 0.0;             // time available per callback
    double cpuLoad = 0.0;              // avg / budget
    uint64_t xrunCount = 0;            // callbacks that exceeded the budget
};

/** Lock-free timing recorder: one writer (audio thread), any readers. */
class LatencyMonitor
{
public:
    void configure (double sampleRate, int bufferSize) noexcept;
    void setDeviceLatency (int inputSamples, int outputSamples) noexcept;
    void reset() noexcept;

    /** Audio thread: record one callback that took durationUs for numSamples. */
    void recordCallback (double durationUs, int numSamples) noexcept;

    LatencyStats getStats() const noexcept;

private:
    std::atomic<double> sampleRate { 0.0 }, lastUs { 0.0 }, totalUs { 0.0 }, maxUs { 0.0 }, budgetUs { 0.0 };
    std::atomic<int> bufferSize { 0 }, inLatency { 0 }, outLatency { 0 };
    std::atomic<uint64_t> count { 0 }, xruns { 0 };
};

class AudioDevice
{
public:
    virtual ~AudioDevice() = default;

    /** Starts streaming into the callback. The callback must outlive stop(). */
    virtual bool start (AudioCallback& callback) = 0;
    /** Stops streaming; guarantees no callback is in flight on return. */
    virtual void stop() = 0;
    virtual bool isRunning() const = 0;

    virtual juce::String getName() const = 0;
    virtual double getSampleRate() const = 0;
    virtual int getBufferSize() const = 0;

    LatencyMonitor& getLatencyMonitor() noexcept { return monitor; }
    LatencyStats getLatencyStats() const noexcept { return monitor.getStats(); }

protected:
    LatencyMonitor monitor;
};

//==============================================================================
/** Real device through juce::AudioDeviceManager. */
class JuceAudioDevice : public AudioDevice, private juce::AudioIODeviceCallback
{
public:
    JuceAudioDevice();
    ~JuceAudioDevice() override;

    /** Opens the default (or saved) device. Returns an error string, empty on success. */
    juce::String initialise (int numInputChannels = 2, int numOutputChannels = 2,
                             const juce::XmlElement* savedState = nullptr);

    juce::AudioDeviceManager& getDeviceManager() noexcept { return deviceManager; }

    bool start (AudioCallback& callback) override;
    void stop() override;
    bool isRunning() const override { return running.load(); }
    juce::String getName() const override;
    double getSampleRate() const override;
    int getBufferSize() const override;

private:
    void audioDeviceIOCallbackWithContext (const float* const*, int, float* const*, int, int,
                                           const juce::AudioIODeviceCallbackContext&) override;
    void audioDeviceAboutToStart (juce::AudioIODevice*) override;
    void audioDeviceStopped() override;

    juce::AudioDeviceManager deviceManager;
    std::atomic<AudioCallback*> userCallback { nullptr };
    std::atomic<bool> running { false };
};

//==============================================================================
/** Headless device. Drive it synchronously with renderBlock(), or start a
    paced thread with startRealtime(). */
class MockAudioDevice : public AudioDevice
{
public:
    MockAudioDevice (double sampleRate = 48000.0, int bufferSize = 256,
                     int numInputs = 2, int numOutputs = 2);
    ~MockAudioDevice() override;

    bool start (AudioCallback& callback) override;
    void stop() override;
    bool isRunning() const override { return running.load(); }
    juce::String getName() const override { return "Mock Audio Device"; }
    double getSampleRate() const override { return sampleRate; }
    int getBufferSize() const override { return bufferSize; }

    void setSimulatedLatency (int inputSamples, int outputSamples);

    /** Input fed to the callback each block (preallocated; fill before rendering). */
    juce::AudioBuffer<float>& getInputBuffer() noexcept { return inputBuffer; }
    /** Output of the most recent block. */
    const juce::AudioBuffer<float>& getLastOutput() const noexcept { return outputBuffer; }

    /** Preallocates a capture buffer holding up to maxFrames of output. */
    void startCapture (int maxFrames);
    const juce::AudioBuffer<float>& getCapture() const noexcept { return capture; }
    int getCapturedFrames() const noexcept { return capturedFrames; }

    /** Runs one callback synchronously on the calling thread. Returns false if not started. */
    bool renderBlock();
    int renderBlocks (int numBlocks);

    /** Runs blocks on a background thread paced at real time. */
    void startRealtime();
    void stopRealtime();

private:
    class PacedThread;

    const double sampleRate;
    const int bufferSize;
    juce::AudioBuffer<float> inputBuffer, outputBuffer, capture;
    int capturedFrames = 0;
    std::vector<const float*> inPtrs;
    std::vector<float*> outPtrs;
    std::atomic<AudioCallback*> callback { nullptr };
    std::atomic<bool> running { false };
    std::unique_ptr<PacedThread> thread;
};

} // namespace openmixer
