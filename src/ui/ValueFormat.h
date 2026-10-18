// Readouts for control values: what the knob means, not its raw 0..1 position.
#pragma once

#include <juce_core/juce_core.h>

#include <cmath>

namespace openmixer::ui::format
{

inline juce::String signedFixed (double v, int decimals)
{
    const auto text = juce::String (v, decimals);
    const double shown = text.getDoubleValue();
    if (shown == 0.0)
        return juce::String (0.0, decimals);   // no "-0.0"
    return (shown > 0.0 ? "+" : "") + text;
}

inline juce::String decibels (double db) { return signedFixed (db, 1) + " dB"; }

// Linear gain shown as dB; silence reads as -inf.
inline juce::String gainAsDecibels (double linear)
{
    return linear <= 0.0 ? juce::String ("-inf dB") : decibels (20.0 * std::log10 (linear));
}

// Sweep filter: -1 low-pass .. 0 off .. +1 high-pass.
inline juce::String filter (double v)
{
    const auto percent = juce::String (juce::roundToInt (std::abs (v) * 100.0)) + "%";
    if (std::abs (v) < 0.005)
        return "Off";
    return (v < 0.0 ? "LPF " : "HPF ") + percent;
}

// Playback rate (1.0 = original tempo) as a pitch-fader percentage.
inline juce::String rate (double r) { return signedFixed ((r - 1.0) * 100.0, 1) + "%"; }

} // namespace openmixer::ui::format
