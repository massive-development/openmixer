// Lock-free callback timing and device latency statistics.
#pragma once

#include <atomic>
#include <cstdint>

namespace openmixer
{

struct LatencyStats
{
    double sampleRate = 0.0;
    int bufferSize = 0;
    int inputLatencySamples = 0;
    int outputLatencySamples = 0;
    double inputLatencyMs = 0.0;
    double outputLatencyMs = 0.0;
    double roundTripLatencyMs = 0.0;
    uint64_t callbackCount = 0;
    double lastCallbackUs = 0.0;
    double avgCallbackUs = 0.0;
    double maxCallbackUs = 0.0;
    double budgetUs = 0.0;
    double cpuLoad = 0.0;              // avg / budget
    uint64_t xrunCount = 0;            // callbacks over budget
};

// One writer (audio thread), any readers.
class LatencyMonitor
{
public:
    void configure (double sampleRate, int bufferSize) noexcept;
    void setDeviceLatency (int inputSamples, int outputSamples) noexcept;
    void reset() noexcept;

    void recordCallback (double durationUs, int numSamples) noexcept;

    LatencyStats getStats() const noexcept;

private:
    std::atomic<double> sampleRate { 0.0 }, lastUs { 0.0 }, totalUs { 0.0 }, maxUs { 0.0 }, budgetUs { 0.0 };
    std::atomic<int> bufferSize { 0 }, inLatency { 0 }, outLatency { 0 };
    std::atomic<uint64_t> count { 0 }, xruns { 0 };
};

} // namespace openmixer
