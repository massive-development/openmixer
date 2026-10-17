// Beat grid seen by sync/quantize. Wave 3 analysis supplies real grids; FixedTempoGrid covers
// constant-tempo material and tests. Positions are source frames; beat 0 is a downbeat.
#pragma once

#include <cstdint>

namespace openmixer
{

class BeatGrid
{
public:
    virtual ~BeatGrid() = default;

    virtual double getBpm() const noexcept = 0;
    virtual int getBeatsPerBar() const noexcept { return 4; }
    virtual double beatAtFrame (std::int64_t frame) const noexcept = 0;
    virtual std::int64_t frameAtBeat (double beat) const noexcept = 0;
};

class FixedTempoGrid final : public BeatGrid
{
public:
    // firstBeatFrame: source frame of beat 0 (may be negative or beyond the start).
    FixedTempoGrid (double bpm, double sourceSampleRate, std::int64_t firstBeatFrame = 0) noexcept
        : tempo (bpm), framesPerBeat (sourceSampleRate * 60.0 / bpm), origin (firstBeatFrame) {}

    double getBpm() const noexcept override { return tempo; }
    double beatAtFrame (std::int64_t frame) const noexcept override
    {
        return static_cast<double> (frame - origin) / framesPerBeat;
    }
    std::int64_t frameAtBeat (double beat) const noexcept override
    {
        return origin + static_cast<std::int64_t> (beat * framesPerBeat + (beat < 0.0 ? -0.5 : 0.5));
    }

private:
    double tempo, framesPerBeat;
    std::int64_t origin;
};

} // namespace openmixer
