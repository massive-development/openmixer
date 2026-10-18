// Design tokens (Todo 25): colours, spacing and type for the glass UI. Message thread only.
// Surfaces are translucent white over the backdrop, so they read correctly over any backdrop,
// including the desktop itself when the window runs semi-transparent.
#pragma once

#include <juce_gui_basics/juce_gui_basics.h>

#include <algorithm>
#include <array>

namespace openmixer::ui::theme
{

// Backdrop: deep blue-black gradient with two soft glows the glass surfaces pick up.
inline const juce::Colour backdropTop { 0xFF0E1220 };
inline const juce::Colour backdropBottom { 0xFF07090F };
inline const juce::Colour glowCool { 0x3038BDF8 };
inline const juce::Colour glowWarm { 0x28F472B6 };

// Glass surfaces, from the card itself to the sunken wells that hold tracks and meters.
inline const juce::Colour surface { 0x14FFFFFF };
inline const juce::Colour surfaceRaised { 0x1FFFFFFF };
inline const juce::Colour surfaceHover { 0x2EFFFFFF };
inline const juce::Colour well { 0x59000000 };
inline const juce::Colour border { 0x1FFFFFFF };
inline const juce::Colour highlight { 0x26FFFFFF };   // top-edge sheen
inline const juce::Colour shadow { 0x66000000 };

inline const juce::Colour textPrimary { 0xF2F4F6FB };
inline const juce::Colour textSecondary { 0xA6C9D1E3 };
inline const juce::Colour textMuted { 0x6BC9D1E3 };

inline const juce::Colour meterLow { 0xFF34D399 };
inline const juce::Colour meterMid { 0xFFFBBF24 };
inline const juce::Colour meterHigh { 0xFFF43F5E };

inline const juce::Colour master { 0xFFE2E8F0 };

// One accent per deck/channel; channels beyond the palette wrap.
inline juce::Colour accentFor (int index)
{
    static const std::array<juce::Colour, 4> accents { juce::Colour { 0xFF38BDF8 }, juce::Colour { 0xFFF472B6 },
                                                       juce::Colour { 0xFFA3E635 }, juce::Colour { 0xFFFBBF24 } };
    const auto n = static_cast<int> (accents.size());
    return accents[static_cast<size_t> (((index % n) + n) % n)];
}

// Spacing scale (px) and shape.
constexpr int kGap = 12, kPad = 12, kInnerGap = 8;
constexpr float kCardRadius = 14.0F, kControlRadius = 8.0F;

inline juce::Font font (float height, bool bold = false)
{
    return juce::Font (juce::FontOptions (height, bold ? juce::Font::bold : juce::Font::plain));
}

inline juce::Font monoFont (float height)
{
    return juce::Font (juce::FontOptions (juce::Font::getDefaultMonospacedFontName(), height, juce::Font::plain));
}

// A frosted card: soft drop shadow, translucent fill, sheen on the top edge, hairline border.
inline void paintGlassCard (juce::Graphics& g, juce::Rectangle<float> r, juce::Colour tint = juce::Colours::transparentBlack)
{
    g.setColour (shadow.withMultipliedAlpha (0.5F));
    g.fillRoundedRectangle (r.translated (0.0F, 3.0F), kCardRadius);

    g.setColour (surface);
    g.fillRoundedRectangle (r, kCardRadius);
    if (! tint.isTransparent())
    {
        g.setGradientFill (juce::ColourGradient (tint.withAlpha (0.12F), r.getX(), r.getY(),
                                                 tint.withAlpha (0.0F), r.getX(), r.getY() + r.getHeight() * 0.6F, false));
        g.fillRoundedRectangle (r, kCardRadius);
    }

    g.setGradientFill (juce::ColourGradient (highlight, r.getX(), r.getY(),
                                             juce::Colours::transparentWhite, r.getX(), r.getY() + 24.0F, false));
    g.fillRoundedRectangle (r.withHeight (std::min (24.0F, r.getHeight())), kCardRadius);

    g.setColour (border);
    g.drawRoundedRectangle (r.reduced (0.5F), kCardRadius, 1.0F);
}

// Full-window backdrop; opacity < 1 lets the desktop show through a semi-transparent window.
inline void paintBackdrop (juce::Graphics& g, juce::Rectangle<float> r, float opacity)
{
    g.setGradientFill (juce::ColourGradient (backdropTop.withMultipliedAlpha (opacity), r.getX(), r.getY(),
                                             backdropBottom.withMultipliedAlpha (opacity), r.getX(), r.getBottom(), false));
    g.fillRect (r);

    const auto radius = std::max (r.getWidth(), r.getHeight()) * 0.55F;
    const auto glow = [&g, radius] (juce::Colour c, juce::Point<float> centre)
    {
        g.setGradientFill (juce::ColourGradient (c, centre, c.withAlpha (0.0F), centre.translated (radius, 0.0F), true));
        g.fillEllipse (juce::Rectangle<float> (radius * 2.0F, radius * 2.0F).withCentre (centre));
    };
    glow (glowCool.withMultipliedAlpha (opacity), { r.getX() + r.getWidth() * 0.15F, r.getY() + r.getHeight() * 0.1F });
    glow (glowWarm.withMultipliedAlpha (opacity), { r.getRight() - r.getWidth() * 0.15F, r.getBottom() - r.getHeight() * 0.15F });
}

} // namespace openmixer::ui::theme
