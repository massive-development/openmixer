#include "Theme.h"

#include <algorithm>

namespace openmixer::ui
{

const ThemeTokens& ThemeTokens::builtinDark()
{
    static const ThemeTokens dark = []
    {
        ThemeTokens t;
        t.backdropTop = juce::Colour (0xFF0E1220);   t.backdropBottom = juce::Colour (0xFF07090F);
        t.glowCool = juce::Colour (0x3038BDF8);      t.glowWarm = juce::Colour (0x28F472B6);
        t.surface = juce::Colour (0x14FFFFFF);       t.surfaceRaised = juce::Colour (0x1FFFFFFF);
        t.surfaceHover = juce::Colour (0x2EFFFFFF);  t.well = juce::Colour (0x59000000);
        t.border = juce::Colour (0x1FFFFFFF);        t.highlight = juce::Colour (0x26FFFFFF);
        t.shadow = juce::Colour (0x66000000);
        t.textPrimary = juce::Colour (0xF2F4F6FB);   t.textSecondary = juce::Colour (0xA6C9D1E3);
        t.textMuted = juce::Colour (0x6BC9D1E3);     t.onAccent = juce::Colour (0xFF07090F);
        t.meterLow = juce::Colour (0xFF34D399);      t.meterMid = juce::Colour (0xFFFBBF24);
        t.meterHigh = juce::Colour (0xFFF43F5E);     t.master = juce::Colour (0xFFE2E8F0);
        t.popup = juce::Colour (0xF0141926);         t.capTop = juce::Colour (0xFF2C3446);
        t.capBottom = juce::Colour (0xFF161B26);
        t.accents = { juce::Colour (0xFF38BDF8), juce::Colour (0xFFF472B6), juce::Colour (0xFFA3E635), juce::Colour (0xFFFBBF24) };
        return t;
    }();
    return dark;
}

} // namespace openmixer::ui

namespace openmixer::ui::theme
{

namespace
{
    struct State
    {
        ThemeTokens tokens = ThemeTokens::builtinDark();
        juce::ListenerList<ThemeListener> listeners;
    };

    State& state()
    {
        static State s;
        return s;
    }
}

const ThemeTokens& current() { return state().tokens; }

void setCurrent (const ThemeTokens& tokens)
{
    auto& s = state();
    s.tokens = tokens;
    if (s.tokens.accents.empty())
        s.tokens.accents = ThemeTokens::builtinDark().accents;   // accentFor must always have a colour
    s.listeners.call ([] (ThemeListener& l) { l.themeChanged(); });
}

void addListener (ThemeListener* listener) { state().listeners.add (listener); }
void removeListener (ThemeListener* listener) { state().listeners.remove (listener); }

juce::Colour accentFor (int index)
{
    const auto& accents = current().accents;
    const auto n = static_cast<int> (accents.size());
    return accents[static_cast<size_t> (((index % n) + n) % n)];
}

juce::Font font (float height, bool bold)
{
    return juce::Font (juce::FontOptions (height, bold ? juce::Font::bold : juce::Font::plain));
}

juce::Font monoFont (float height)
{
    return juce::Font (juce::FontOptions (juce::Font::getDefaultMonospacedFontName(), height, juce::Font::plain));
}

void paintGlassCard (juce::Graphics& g, juce::Rectangle<float> r, juce::Colour tint)
{
    const float radius = cardRadius();
    g.setColour (shadow().withMultipliedAlpha (0.5F));
    g.fillRoundedRectangle (r.translated (0.0F, 3.0F), radius);

    g.setColour (surface());
    g.fillRoundedRectangle (r, radius);
    if (! tint.isTransparent())
    {
        g.setGradientFill (juce::ColourGradient (tint.withAlpha (0.12F), r.getX(), r.getY(),
                                                 tint.withAlpha (0.0F), r.getX(), r.getY() + r.getHeight() * 0.6F, false));
        g.fillRoundedRectangle (r, radius);
    }

    g.setGradientFill (juce::ColourGradient (highlight(), r.getX(), r.getY(),
                                             highlight().withAlpha (0.0F), r.getX(), r.getY() + 24.0F, false));
    g.fillRoundedRectangle (r.withHeight (std::min (24.0F, r.getHeight())), radius);

    g.setColour (border());
    g.drawRoundedRectangle (r.reduced (0.5F), radius, 1.0F);
}

void paintBackdrop (juce::Graphics& g, juce::Rectangle<float> r, float opacity)
{
    opacity *= current().backdropOpacity;
    g.setGradientFill (juce::ColourGradient (backdropTop().withMultipliedAlpha (opacity), r.getX(), r.getY(),
                                             backdropBottom().withMultipliedAlpha (opacity), r.getX(), r.getBottom(), false));
    g.fillRect (r);

    const auto radius = std::max (r.getWidth(), r.getHeight()) * 0.55F;
    const auto glow = [&g, radius] (juce::Colour c, juce::Point<float> centre)
    {
        g.setGradientFill (juce::ColourGradient (c, centre, c.withAlpha (0.0F), centre.translated (radius, 0.0F), true));
        g.fillEllipse (juce::Rectangle<float> (radius * 2.0F, radius * 2.0F).withCentre (centre));
    };
    glow (glowCool().withMultipliedAlpha (opacity), { r.getX() + r.getWidth() * 0.15F, r.getY() + r.getHeight() * 0.1F });
    glow (glowWarm().withMultipliedAlpha (opacity), { r.getRight() - r.getWidth() * 0.15F, r.getBottom() - r.getHeight() * 0.15F });
}

} // namespace openmixer::ui::theme
