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

    // Mixer metrics (px) and meter ballistics (dB / seconds); meterClipLevel is a linear peak.
    float mixerKnobHeight = 70.0F, mixerButtonHeight = 26.0F, mixerMeterWidth = 12.0F, mixerMasterWidth = 92.0F;
    float mixerTabHeight = 28.0F, mixerCrossfaderHeight = 46.0F, mixerCueHeight = 46.0F;
    float meterFloorDb = -48.0F, meterMidDb = -18.0F, meterHighDb = -6.0F, meterClipLevel = 0.99F;
    float meterReleaseSeconds = 0.3F, meterHoldSeconds = 1.0F, meterClipSeconds = 2.0F;
    float mixerTitleHeight = 22.0F, mixerFaderWidth = 40.0F, mixerCurveWidth = 128.0F, mixerFxBoxWidth = 150.0F;
    float mixerFxEnableWidth = 56.0F, mixerPageButtonWidth = 80.0F, mixerPadX = 6.0F, mixerPadY = 10.0F, mixerAccentDot = 8.0F;
    float meterSegment = 4.0F, meterSegmentGap = 1.0F, meterHoldTickHeight = 2.0F;

    // Library browser metrics (px) and row tint strengths (0..1 alpha).
    float libraryRowHeight = 26.0F, libraryHeaderHeight = 26.0F, librarySidebarWidth = 190.0F, libraryToolbarHeight = 34.0F;
    float libraryButtonWidth = 78.0F, librarySidebarRowHeight = 24.0F, libraryRatingDot = 5.0F, librarySearchMinWidth = 160.0F;
    float libraryCompatAlpha = 0.22F, libraryStripeAlpha = 0.04F, libraryDropAlpha = 0.18F;

    float surfaceOpacity = 1.0F;   // scales the alpha of surface/raised/hover (glass strength)
    float backdropOpacity = 1.0F;  // default backdrop opacity before a window asks for less

    // The built-in dark theme; also the fallback for anything a theme file leaves out.
    static const ThemeTokens& builtinDark();
};

} // namespace openmixer::ui
