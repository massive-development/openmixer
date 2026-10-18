#include "WaveformGeometry.h"

#include <algorithm>

namespace openmixer::ui::wave
{

double xToFraction (double x, double width) noexcept
{
    return width > 0.0 ? std::clamp (x / width, 0.0, 1.0) : 0.0;
}

double frameToX (std::int64_t frame, std::int64_t length, double width) noexcept
{
    return length > 0 ? width * static_cast<double> (frame) / static_cast<double> (length) : 0.0;
}

double View::xOfFrame (std::int64_t frame) const noexcept
{
    return width * 0.5 + static_cast<double> (frame - centreFrame) / framesPerPixel;
}

double View::frameAtX (double x) const noexcept
{
    return static_cast<double> (centreFrame) + (x - width * 0.5) * framesPerPixel;
}

double clampZoom (double framesPerPixel) noexcept
{
    return std::clamp (framesPerPixel, kMinFramesPerPixel, kMaxFramesPerPixel);
}

} // namespace openmixer::ui::wave
