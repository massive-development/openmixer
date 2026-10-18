#include "WaveformPainter.h"

#include "Theme.h"

#include <algorithm>
#include <cmath>

namespace openmixer::ui::wave
{

namespace
{
    constexpr float kMinEnergy = 1.0e-4F;   // below this a column is silence: avoid dividing by ~0

    struct Column { float min = 0.0F, max = 0.0F; analysis::WaveformPoint bands; };

    // Merge the points covered by one pixel: widest peak range, loudest band of each.
    Column mergePoints (const analysis::WaveformLevel& level, double from, double to)
    {
        const auto count = static_cast<long long> (level.points.size());
        auto i0 = std::clamp (static_cast<long long> (std::floor (from / level.samplesPerPoint)), 0LL, count - 1);
        auto i1 = std::clamp (static_cast<long long> (std::ceil (to / level.samplesPerPoint)), i0 + 1, count);
        Column c;
        for (auto i = i0; i < i1; ++i)
        {
            const auto& p = level.points[static_cast<size_t> (i)];
            c.min = std::min (c.min, p.min);
            c.max = std::max (c.max, p.max);
            c.bands.low = std::max (c.bands.low, p.low);
            c.bands.mid = std::max (c.bands.mid, p.mid);
            c.bands.high = std::max (c.bands.high, p.high);
        }
        return c;
    }
}

juce::Colour bandColour (const analysis::WaveformPoint& p)
{
    const float sum = p.low + p.mid + p.high;
    if (sum < kMinEnergy)
        return theme::waveLow();
    const auto lowMid = theme::waveLow().interpolatedWith (theme::waveMid(), p.mid / (p.low + p.mid + kMinEnergy));
    return lowMid.interpolatedWith (theme::waveHigh(), p.high / sum);
}

void paintColumns (juce::Graphics& g, const analysis::WaveformData& data, juce::Rectangle<float> area,
                   double firstSample, double lastSample)
{
    if (! data.isValid() || area.isEmpty() || lastSample <= firstSample)
        return;
    const double perPixel = (lastSample - firstSample) / static_cast<double> (area.getWidth());
    const auto& level = data.levelFor (perPixel);
    const float mid = area.getCentreY(), half = area.getHeight() * 0.5F;
    for (float x = 0.0F; x < area.getWidth(); x += 1.0F)
    {
        const double s0 = firstSample + x * perPixel;
        if (s0 < 0.0 || s0 >= static_cast<double> (data.getNumSamples()))
            continue;   // outside the track: leave blank
        const auto c = mergePoints (level, s0, s0 + perPixel);
        g.setColour (bandColour (c.bands));
        g.drawVerticalLine (juce::roundToInt (area.getX() + x), mid - c.max * half, std::max (mid - c.min * half, mid - c.max * half + 1.0F));
    }
}

void paintEmpty (juce::Graphics& g, juce::Rectangle<float> area, const juce::String& text)
{
    g.setColour (theme::textMuted());
    g.setFont (theme::font (theme::typeCaption()));
    g.drawText (text, area, juce::Justification::centred);
}

} // namespace openmixer::ui::wave
