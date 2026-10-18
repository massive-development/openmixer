// Pure mapping between pixels and deck frames for the overview and zoomed waveforms.
// Kept free of Components so seek-by-click and scrolling can be tested without painting.
#pragma once

#include <cstdint>

namespace openmixer::ui::wave
{

// Overview: the whole track spans the width.
double xToFraction (double x, double width) noexcept;                       // clamped to 0..1
double frameToX (std::int64_t frame, std::int64_t length, double width) noexcept;

// Zoomed view: centred on the playhead, framesPerPixel sets the scale.
struct View
{
    std::int64_t centreFrame = 0;
    double framesPerPixel = 1.0;
    double width = 1.0;

    double xOfFrame (std::int64_t frame) const noexcept;
    double frameAtX (double x) const noexcept;
    double firstFrame() const noexcept { return frameAtX (0.0); }
    double lastFrame() const noexcept { return frameAtX (width); }
};

constexpr double kMinFramesPerPixel = 32.0, kMaxFramesPerPixel = 8192.0;
double clampZoom (double framesPerPixel) noexcept;

} // namespace openmixer::ui::wave
