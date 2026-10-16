#include "LatencyMonitor.h"

namespace openmixer
{

void LatencyMonitor::configure (double sr, int bs) noexcept
{
    sampleRate.store (sr, std::memory_order_relaxed);
    bufferSize.store (bs, std::memory_order_relaxed);
    budgetUs.store (sr > 0.0 ? 1.0e6 * bs / sr : 0.0, std::memory_order_relaxed);
}

void LatencyMonitor::setDeviceLatency (int i, int o) noexcept
{
    inLatency.store (i, std::memory_order_relaxed);
    outLatency.store (o, std::memory_order_relaxed);
}

void LatencyMonitor::reset() noexcept
{
    lastUs.store (0.0, std::memory_order_relaxed);
    totalUs.store (0.0, std::memory_order_relaxed);
    maxUs.store (0.0, std::memory_order_relaxed);
    count.store (0, std::memory_order_relaxed);
    xruns.store (0, std::memory_order_relaxed);
}

void LatencyMonitor::recordCallback (double durationUs, int numSamples) noexcept
{
    // Single writer: plain load/store on atomics is sufficient.
    lastUs.store (durationUs, std::memory_order_relaxed);
    totalUs.store (totalUs.load (std::memory_order_relaxed) + durationUs, std::memory_order_relaxed);
    if (durationUs > maxUs.load (std::memory_order_relaxed))
        maxUs.store (durationUs, std::memory_order_relaxed);

    const double sr = sampleRate.load (std::memory_order_relaxed);
    const double budget = sr > 0.0 ? 1.0e6 * numSamples / sr : budgetUs.load (std::memory_order_relaxed);
    if (budget > 0.0 && durationUs > budget)
        xruns.fetch_add (1, std::memory_order_relaxed);
    count.fetch_add (1, std::memory_order_release);
}

LatencyStats LatencyMonitor::getStats() const noexcept
{
    LatencyStats s;
    s.callbackCount = count.load (std::memory_order_acquire);
    s.sampleRate = sampleRate.load (std::memory_order_relaxed);
    s.bufferSize = bufferSize.load (std::memory_order_relaxed);
    s.inputLatencySamples = inLatency.load (std::memory_order_relaxed);
    s.outputLatencySamples = outLatency.load (std::memory_order_relaxed);
    if (s.sampleRate > 0.0)
    {
        s.inputLatencyMs = 1000.0 * s.inputLatencySamples / s.sampleRate;
        s.outputLatencyMs = 1000.0 * s.outputLatencySamples / s.sampleRate;
        s.roundTripLatencyMs = 1000.0 * (s.bufferSize + s.inputLatencySamples + s.outputLatencySamples) / s.sampleRate;
    }
    s.lastCallbackUs = lastUs.load (std::memory_order_relaxed);
    s.maxCallbackUs = maxUs.load (std::memory_order_relaxed);
    s.avgCallbackUs = s.callbackCount ? totalUs.load (std::memory_order_relaxed) / static_cast<double> (s.callbackCount) : 0.0;
    s.budgetUs = budgetUs.load (std::memory_order_relaxed);
    s.cpuLoad = s.budgetUs > 0.0 ? s.avgCallbackUs / s.budgetUs : 0.0;
    s.xrunCount = xruns.load (std::memory_order_relaxed);
    return s;
}

} // namespace openmixer
