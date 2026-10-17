// Key-lock time-stretch: a realtime Rubber Band stretcher behind a small RT-safe interface.
// prepare() allocates (control thread); everything else is audio-thread only and allocation-free.
#pragma once

#include <memory>

namespace RubberBand { class RubberBandStretcher; }

namespace openmixer
{

class TimeStretch
{
public:
    TimeStretch();
    ~TimeStretch();

    TimeStretch (const TimeStretch&) = delete;
    TimeStretch& operator= (const TimeStretch&) = delete;

    void prepare (double sampleRate, int maxBlockSize);
    bool isPrepared() const noexcept { return stretcher != nullptr; }

    // Clears state and feeds the preferred start pad; returns output samples to discard.
    int restart() noexcept;
    // timeRatio = output/input length, pitchScale = output/input pitch.
    void setRatios (double timeRatio, double pitchScale) noexcept;

    int getSamplesRequired() const noexcept;
    int getMaxFeed() const noexcept { return maxFeed; }
    void process (const float* const* stereo, int numFrames) noexcept;
    int getAvailable() const noexcept;
    int retrieve (float* const* stereo, int numFrames) noexcept;
    int getStartDelay() const noexcept;

private:
    std::unique_ptr<RubberBand::RubberBandStretcher> stretcher;
    std::unique_ptr<float[]> zeros;
    int maxFeed = 0;
};

} // namespace openmixer
