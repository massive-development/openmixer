// Draws WaveformData columns coloured by the low/mid/high bands. Shared by the overview and zoom views.
#pragma once

#include "../analysis/WaveformData.h"

#include <juce_graphics/juce_graphics.h>

namespace openmixer::ui::wave
{

// Colour for one point: the three theme waveform colours blended by band energy.
juce::Colour bandColour (const analysis::WaveformPoint& p);

// Paints [firstSample, lastSample) of the waveform across `area`, one column per pixel, using the
// coarsest level with a point per pixel.
void paintColumns (juce::Graphics& g, const analysis::WaveformData& data, juce::Rectangle<float> area,
                   double firstSample, double lastSample);

// Placeholder drawn when a deck has no waveform yet.
void paintEmpty (juce::Graphics& g, juce::Rectangle<float> area, const juce::String& text);

} // namespace openmixer::ui::wave
