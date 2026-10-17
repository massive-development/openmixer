#include "TimeStretch.h"

#include <rubberband/RubberBandStretcher.h>

#include <algorithm>

namespace openmixer
{

namespace
{
    using RB = RubberBand::RubberBandStretcher;
    constexpr RB::Options kOptions = RB::OptionProcessRealTime | RB::OptionEngineFiner
                                   | RB::OptionChannelsTogether | RB::OptionPitchHighConsistency;
}

TimeStretch::TimeStretch() = default;
TimeStretch::~TimeStretch() = default;

void TimeStretch::prepare (double sampleRate, int maxBlockSize)
{
    stretcher = std::make_unique<RB> (static_cast<size_t> (sampleRate), 2, kOptions);
    maxFeed = std::max (4096, maxBlockSize * 8);   // covers getSamplesRequired() at extreme ratios
    stretcher->setMaxProcessSize (static_cast<size_t> (maxFeed));
    zeros = std::make_unique<float[]> (static_cast<size_t> (maxFeed));
    std::fill_n (zeros.get(), maxFeed, 0.0F);
}

int TimeStretch::restart() noexcept
{
    stretcher->reset();
    auto pad = static_cast<int> (stretcher->getPreferredStartPad());
    const float* silent[2] { zeros.get(), zeros.get() };
    for (; pad > 0; pad -= maxFeed)
        stretcher->process (silent, static_cast<size_t> (std::min (pad, maxFeed)), false);
    return getStartDelay();
}

void TimeStretch::setRatios (double timeRatio, double pitchScale) noexcept
{
    if (stretcher->getTimeRatio() != timeRatio)
        stretcher->setTimeRatio (timeRatio);
    if (stretcher->getPitchScale() != pitchScale)
        stretcher->setPitchScale (pitchScale);
}

int TimeStretch::getSamplesRequired() const noexcept
{
    return std::clamp (static_cast<int> (stretcher->getSamplesRequired()), 0, maxFeed);
}

void TimeStretch::process (const float* const* stereo, int numFrames) noexcept
{
    stretcher->process (stereo, static_cast<size_t> (numFrames), false);
}

int TimeStretch::getAvailable() const noexcept
{
    return std::max (0, stretcher->available());
}

int TimeStretch::retrieve (float* const* stereo, int numFrames) noexcept
{
    return static_cast<int> (stretcher->retrieve (stereo, static_cast<size_t> (numFrames)));
}

int TimeStretch::getStartDelay() const noexcept
{
    return static_cast<int> (stretcher->getStartDelay());
}

} // namespace openmixer
