// Theme token set (Todo 25/25a): plain data for colours, spacing, shape, type scale and translucency.
// Surfaces are translucent so they read over any backdrop, including a see-through window.
#pragma once

#include <juce_graphics/juce_graphics.h>

#include <vector>

namespace openmixer::ui
{

struct ThemeTokens
{
    // Backdrop: gradient plus two soft glows the glass surfaces pick up.
    juce::Colour backdropTop, backdropBottom, glowCool, glowWarm;
    // Glass surfaces, from the card itself to the sunken wells that hold tracks and meters.
    juce::Colour surface, surfaceRaised, surfaceHover, well, border, highlight, shadow;
    juce::Colour textPrimary, textSecondary, textMuted, onAccent;   // onAccent: text drawn on an accent fill
    juce::Colour meterLow, meterMid, meterHigh, master;
    juce::Colour waveLow, waveMid, waveHigh;                         // waveform band colours (bass / mids / highs)
    juce::Colour popup, capTop, capBottom;                           // menus/tooltips and knob/fader caps

    std::vector<juce::Colour> accents;                               // one per deck/channel, wraps

    int gap = 12, pad = 12, innerGap = 8;                            // spacing scale (px)
    float cardRadius = 14.0F, controlRadius = 8.0F;
    float typeSmall = 11.0F, typeCaption = 12.0F, typeBody = 13.0F, typeLabel = 14.0F;
    float typeTitle = 16.0F, typeHeading = 17.0F, typeMono = 15.0F;  // type scale (px height)

    // Deck widget metrics (px unless noted) and overlay strengths (0..1 alpha).
    float deckHeaderHeight = 52.0F, deckOverviewHeight = 40.0F, deckTransportHeight = 32.0F, deckRowHeight = 30.0F;
    float deckPitchWidth = 84.0F, deckMinZoomHeight = 60.0F, deckReadoutWidth = 78.0F, deckRangeBoxHeight = 24.0F;
    float playheadWidth = 2.0F, cueMarkerWidth = 2.0F;
    float gridBarAlpha = 0.55F, gridBeatAlpha = 0.4F, loopActiveAlpha = 0.25F, loopIdleAlpha = 0.08F;

    float surfaceOpacity = 1.0F;   // scales the alpha of surface/raised/hover (glass strength)
    float backdropOpacity = 1.0F;  // default backdrop opacity before a window asks for less

    // The built-in dark theme; also the fallback for anything a theme file leaves out.
    static const ThemeTokens& builtinDark();
};

} // namespace openmixer::ui
