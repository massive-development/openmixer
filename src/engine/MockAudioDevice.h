#pragma once

#include "AudioDevice.h"

#include <juce_audio_basics/juce_audio_basics.h>

#include <atomic>
#include <memory>
#include <vector>

namespace openmixer
{

//==============================================================================
// Headless device for tests: drive with renderBlock() or a paced thread.
class MockAudioDevice : public AudioDevice
{
public:
    MockAudioDevice (double sampleRate = 48000.0, int bufferSize = 256,
                     int numInputs = 2, int numOutputs = 2);
    ~MockAudioDevice() override;

    bool start (AudioCallback& callback) override;
    void stop() override;
    bool isRunning() const override { return running.load (std::memory_order_relaxed); }
    juce::String getName() const override { return "Mock Audio Device"; }
    double getSampleRate() const override { return sampleRate; }
    int getBufferSize() const override { return bufferSize; }

    void setSimulatedLatency (int inputSamples, int outputSamples);

    juce::AudioBuffer<float>& getInputBuffer() noexcept { return inputBuffer; }
    const juce::AudioBuffer<float>& getLastOutput() const noexcept { return outputBuffer; }

    void startCapture (int maxFrames);
    const juce::AudioBuffer<float>& getCapture() const noexcept { return capture; }
    int getCapturedFrames() const noexcept { return capturedFrames; }

    bool renderBlock();
    int renderBlocks (int numBlocks);

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
