// Design tokens (Todo 25): runtime theme read through accessors, so a theme switch reaches every widget.
// The current theme is a ThemeTokens value (built-in dark until ThemeRegistry selects another).
// Widgets read tokens when painting, and re-apply cached ones (fonts, accents) in lookAndFeelChanged().
// Message thread only.
#pragma once

#include "ThemeTokens.h"

#include <juce_gui_basics/juce_gui_basics.h>

namespace openmixer::ui::theme
{

// Told after the current theme was replaced; the Glass look-and-feel uses it to repaint all windows.
class ThemeListener
{
public:
    virtual ~ThemeListener() = default;
    virtual void themeChanged() = 0;
};

const ThemeTokens& current();
void setCurrent (const ThemeTokens& tokens);
void addListener (ThemeListener* listener);
void removeListener (ThemeListener* listener);

// Backdrop and glow.
inline juce::Colour backdropTop() { return current().backdropTop; }
inline juce::Colour backdropBottom() { return current().backdropBottom; }
inline juce::Colour glowCool() { return current().glowCool; }
inline juce::Colour glowWarm() { return current().glowWarm; }

// Glass surfaces (alpha follows the theme's surface opacity).
inline juce::Colour surface() { return current().surface.withMultipliedAlpha (current().surfaceOpacity); }
inline juce::Colour surfaceRaised() { return current().surfaceRaised.withMultipliedAlpha (current().surfaceOpacity); }
inline juce::Colour surfaceHover() { return current().surfaceHover.withMultipliedAlpha (current().surfaceOpacity); }
inline juce::Colour well() { return current().well; }
inline juce::Colour border() { return current().border; }
inline juce::Colour highlight() { return current().highlight; }   // top-edge sheen
inline juce::Colour shadow() { return current().shadow; }

inline juce::Colour textPrimary() { return current().textPrimary; }
inline juce::Colour textSecondary() { return current().textSecondary; }
inline juce::Colour textMuted() { return current().textMuted; }
inline juce::Colour onAccent() { return current().onAccent; }

inline juce::Colour meterLow() { return current().meterLow; }
inline juce::Colour meterMid() { return current().meterMid; }
inline juce::Colour meterHigh() { return current().meterHigh; }
inline juce::Colour master() { return current().master; }

inline juce::Colour waveLow() { return current().waveLow; }
inline juce::Colour waveMid() { return current().waveMid; }
inline juce::Colour waveHigh() { return current().waveHigh; }

inline juce::Colour popup() { return current().popup; }
inline juce::Colour capTop() { return current().capTop; }
inline juce::Colour capBottom() { return current().capBottom; }

// One accent per deck/channel; channels beyond the palette wrap.
juce::Colour accentFor (int index);

// Spacing scale (px) and shape.
inline int gap() { return current().gap; }
inline int pad() { return current().pad; }
inline int innerGap() { return current().innerGap; }
inline float cardRadius() { return current().cardRadius; }
inline float controlRadius() { return current().controlRadius; }

// Type scale.
inline float typeSmall() { return current().typeSmall; }
inline float typeCaption() { return current().typeCaption; }
inline float typeBody() { return current().typeBody; }
inline float typeLabel() { return current().typeLabel; }
inline float typeTitle() { return current().typeTitle; }
inline float typeHeading() { return current().typeHeading; }
inline float typeMono() { return current().typeMono; }

// Deck widget metrics and overlay strengths.
inline int deckHeaderHeight() { return juce::roundToInt (current().deckHeaderHeight); }
inline int deckOverviewHeight() { return juce::roundToInt (current().deckOverviewHeight); }
inline int deckTransportHeight() { return juce::roundToInt (current().deckTransportHeight); }
inline int deckRowHeight() { return juce::roundToInt (current().deckRowHeight); }
inline int deckPitchWidth() { return juce::roundToInt (current().deckPitchWidth); }
inline int deckMinZoomHeight() { return juce::roundToInt (current().deckMinZoomHeight); }
inline int deckReadoutWidth() { return juce::roundToInt (current().deckReadoutWidth); }
inline int deckRangeBoxHeight() { return juce::roundToInt (current().deckRangeBoxHeight); }
inline float playheadWidth() { return current().playheadWidth; }
inline float cueMarkerWidth() { return current().cueMarkerWidth; }
inline float gridBarAlpha() { return current().gridBarAlpha; }
inline float gridBeatAlpha() { return current().gridBeatAlpha; }
inline float loopActiveAlpha() { return current().loopActiveAlpha; }
inline float loopIdleAlpha() { return current().loopIdleAlpha; }

juce::Font font (float height, bool bold = false);
juce::Font monoFont (float height);

// A frosted card: soft drop shadow, translucent fill, sheen on the top edge, hairline border.
void paintGlassCard (juce::Graphics& g, juce::Rectangle<float> r, juce::Colour tint = juce::Colours::transparentBlack);

// Full-window backdrop; opacity < 1 lets the desktop show through a semi-transparent window.
void paintBackdrop (juce::Graphics& g, juce::Rectangle<float> r, float opacity);

} // namespace openmixer::ui::theme
