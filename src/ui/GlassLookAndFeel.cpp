#include "GlassLookAndFeel.h"

#include "Theme.h"

namespace openmixer::ui
{

namespace
{
    constexpr float kRingThickness = 3.0F, kTrackWidth = 4.0F;
    constexpr int kThumbRadius = 10, kTicks = 11;

    juce::LookAndFeel_V4::ColourScheme glassScheme()
    {
        using namespace theme;
        return { backdropTop(),  surfaceRaised(), popup(),           border(),       textPrimary(),
                 surfaceHover(), textPrimary(),   accentFor (0).withAlpha (0.35F),  textPrimary() };
    }

    void strokeArc (juce::Graphics& g, juce::Point<float> centre, float radius, float from, float to, float thickness)
    {
        juce::Path arc;
        arc.addCentredArc (centre.x, centre.y, radius, radius, 0.0F, from, to, true);
        g.strokePath (arc, juce::PathStrokeType (thickness, juce::PathStrokeType::curved, juce::PathStrokeType::rounded));
    }
}

GlassLookAndFeel::GlassLookAndFeel() : juce::LookAndFeel_V4 (glassScheme())
{
    applyTheme();
    theme::addListener (this);
}

GlassLookAndFeel::~GlassLookAndFeel() { theme::removeListener (this); }

// Re-reads every token into colour ids; custom drawing reads tokens live, so it needs no refresh.
void GlassLookAndFeel::applyTheme()
{
    using namespace theme;
    setColourScheme (glassScheme());   // resets the base class's colour ids; ours follow
    setColour (juce::ResizableWindow::backgroundColourId, backdropTop());
    setColour (juce::DocumentWindow::textColourId, textSecondary());
    setColour (juce::Label::textColourId, textSecondary());
    applyControlColours();
    applyContainerColours();
}

void GlassLookAndFeel::applyControlColours()
{
    using namespace theme;
    setColour (juce::Slider::rotarySliderFillColourId, accentFor (0));
    setColour (juce::Slider::rotarySliderOutlineColourId, surfaceRaised());
    setColour (juce::Slider::trackColourId, accentFor (0));
    setColour (juce::Slider::backgroundColourId, well());
    setColour (juce::Slider::thumbColourId, textPrimary());
    setColour (juce::Slider::textBoxTextColourId, textPrimary());
    setColour (juce::Slider::textBoxBackgroundColourId, juce::Colours::transparentBlack);
    setColour (juce::Slider::textBoxOutlineColourId, juce::Colours::transparentBlack);

    setColour (juce::TextButton::buttonColourId, surfaceRaised());
    setColour (juce::TextButton::buttonOnColourId, accentFor (0));
    setColour (juce::TextButton::textColourOffId, textSecondary());
    setColour (juce::TextButton::textColourOnId, onAccent());
    setColour (juce::ToggleButton::textColourId, textPrimary());
    setColour (juce::ToggleButton::tickColourId, accentFor (0));
    setColour (juce::ToggleButton::tickDisabledColourId, textMuted());
}

void GlassLookAndFeel::applyContainerColours()
{
    using namespace theme;
    setColour (juce::ComboBox::backgroundColourId, well());
    setColour (juce::ComboBox::outlineColourId, border());
    setColour (juce::ComboBox::textColourId, textPrimary());
    setColour (juce::ComboBox::arrowColourId, textSecondary());
    setColour (juce::ComboBox::focusedOutlineColourId, accentFor (0));
    setColour (juce::PopupMenu::backgroundColourId, popup());
    setColour (juce::PopupMenu::textColourId, textPrimary());
    setColour (juce::PopupMenu::headerTextColourId, textMuted());
    setColour (juce::PopupMenu::highlightedBackgroundColourId, accentFor (0).withAlpha (0.35F));
    setColour (juce::PopupMenu::highlightedTextColourId, textPrimary());
    setColour (juce::TextEditor::backgroundColourId, well());
    setColour (juce::TextEditor::textColourId, textPrimary());
    setColour (juce::TextEditor::outlineColourId, border());
    setColour (juce::TextEditor::focusedOutlineColourId, accentFor (0));
    setColour (juce::TextEditor::highlightColourId, accentFor (0).withAlpha (0.35F));
    setColour (juce::TextEditor::highlightedTextColourId, textPrimary());
    setColour (juce::CaretComponent::caretColourId, accentFor (0));
    setColour (juce::ScrollBar::thumbColourId, textMuted());
    setColour (juce::ScrollBar::trackColourId, juce::Colours::transparentBlack);
    setColour (juce::TabbedButtonBar::tabOutlineColourId, border());
    setColour (juce::TabbedButtonBar::frontOutlineColourId, accentFor (0));
    setColour (juce::TabbedComponent::backgroundColourId, juce::Colours::transparentBlack);
    setColour (juce::TableHeaderComponent::textColourId, textSecondary());
    setColour (juce::TableHeaderComponent::backgroundColourId, surfaceRaised());
    setColour (juce::TableHeaderComponent::outlineColourId, border());
    setColour (juce::TableHeaderComponent::highlightColourId, surfaceHover());
    setColour (juce::TooltipWindow::backgroundColourId, popup());
    setColour (juce::TooltipWindow::textColourId, textPrimary());
    setColour (juce::TooltipWindow::outlineColourId, border());
}

void GlassLookAndFeel::themeChanged()
{
    applyTheme();
    auto& desktop = juce::Desktop::getInstance();
    for (int i = 0; i < desktop.getNumComponents(); ++i)
        if (auto* top = desktop.getComponent (i))
            top->sendLookAndFeelChange();   // repaints each window and re-runs lookAndFeelChanged()
}

double GlassLookAndFeel::originOf (const juce::Slider& s)
{
    return s.isDoubleClickReturnEnabled() ? s.getDoubleClickReturnValue() : s.getMinimum();
}

void GlassLookAndFeel::drawRotarySlider (juce::Graphics& g, int x, int y, int width, int height, float sliderPos,
                                         float rotaryStartAngle, float rotaryEndAngle, juce::Slider& slider)
{
    using namespace theme;
    const auto bounds = juce::Rectangle<int> (x, y, width, height).toFloat().reduced (2.0F);
    const float radius = std::min (bounds.getWidth(), bounds.getHeight()) * 0.5F - kRingThickness;
    if (radius <= 4.0F)
        return;
    const auto centre = bounds.getCentre();
    const auto accent = slider.findColour (juce::Slider::rotarySliderFillColourId);
    const auto angleAt = [rotaryStartAngle, rotaryEndAngle] (float p) { return rotaryStartAngle + p * (rotaryEndAngle - rotaryStartAngle); };
    const float origin = angleAt (static_cast<float> (slider.valueToProportionOfLength (originOf (slider))));
    const float angle = angleAt (sliderPos);
    const bool live = slider.isMouseOverOrDragging() && slider.isEnabled();

    g.setColour (well());
    strokeArc (g, centre, radius, rotaryStartAngle, rotaryEndAngle, kRingThickness);
    if (std::abs (angle - origin) > 0.001F)
    {
        if (live)
        {
            g.setColour (accent.withAlpha (0.25F));
            strokeArc (g, centre, radius, std::min (origin, angle), std::max (origin, angle), kRingThickness * 2.5F);
        }
        g.setColour (accent);
        strokeArc (g, centre, radius, std::min (origin, angle), std::max (origin, angle), kRingThickness);
    }

    // Cap: a domed disc with a hairline rim, inset from the ring.
    const float capRadius = radius - kRingThickness - 3.0F;
    const auto cap = juce::Rectangle<float> (capRadius * 2.0F, capRadius * 2.0F).withCentre (centre);
    g.setColour (shadow());
    g.fillEllipse (cap.translated (0.0F, 2.0F));
    g.setGradientFill (juce::ColourGradient (live ? capTop().brighter (0.15F) : capTop(), cap.getX(), cap.getY(),
                                             capBottom(), cap.getX(), cap.getBottom(), false));
    g.fillEllipse (cap);
    g.setColour (highlight());
    g.drawEllipse (cap.reduced (0.5F), 1.0F);

    const auto tip = centre.getPointOnCircumference (capRadius * 0.8F, angle);
    const auto base = centre.getPointOnCircumference (capRadius * 0.3F, angle);
    g.setColour (textPrimary());
    g.drawLine ({ base, tip }, 2.5F);
}

int GlassLookAndFeel::getSliderThumbRadius (juce::Slider& slider)
{
    return slider.isHorizontal() ? std::min (kThumbRadius, slider.getHeight() / 2) : std::min (kThumbRadius, slider.getWidth() / 2);
}

void GlassLookAndFeel::drawLinearSlider (juce::Graphics& g, int x, int y, int width, int height, float sliderPos,
                                         float minSliderPos, float maxSliderPos, juce::Slider::SliderStyle style,
                                         juce::Slider& slider)
{
    if (style != juce::Slider::LinearVertical && style != juce::Slider::LinearHorizontal)
    {
        juce::LookAndFeel_V4::drawLinearSlider (g, x, y, width, height, sliderPos, minSliderPos, maxSliderPos, style, slider);
        return;
    }

    using namespace theme;
    const bool vertical = style == juce::Slider::LinearVertical;
    const auto area = juce::Rectangle<int> (x, y, width, height).toFloat();
    const auto accent = slider.findColour (juce::Slider::trackColourId);
    const float originPos = static_cast<float> (slider.getPositionOfValue (originOf (slider)));

    // Scale ticks either side of the groove, every tenth of travel.
    const float along0 = vertical ? area.getBottom() : area.getX(), along1 = vertical ? area.getY() : area.getRight();
    const float across = vertical ? area.getCentreX() : area.getCentreY();
    const float span = vertical ? area.getWidth() : area.getHeight();
    g.setColour (textMuted());
    for (int i = 0; i < kTicks; ++i)
    {
        const float p = juce::jmap (static_cast<float> (i) / (kTicks - 1), along0, along1);
        const float len = (i == 0 || i == kTicks - 1 || i == kTicks / 2) ? 6.0F : 3.5F;
        for (const float side : { -1.0F, 1.0F })
        {
            const float a = across + side * std::min (span * 0.5F - 1.0F, 14.0F);
            const float b = a - side * len;
            vertical ? g.drawLine (a, p, b, p, 1.0F) : g.drawLine (p, a, p, b, 1.0F);
        }
    }

    const auto groove = vertical ? juce::Rectangle<float> (across - kTrackWidth * 0.5F, area.getY(), kTrackWidth, area.getHeight())
                                 : juce::Rectangle<float> (area.getX(), across - kTrackWidth * 0.5F, area.getWidth(), kTrackWidth);
    g.setColour (well());
    g.fillRoundedRectangle (groove, kTrackWidth * 0.5F);

    const float lo = std::min (originPos, sliderPos), hi = std::max (originPos, sliderPos);
    const auto fill = vertical ? groove.withY (lo).withHeight (hi - lo) : groove.withX (lo).withWidth (hi - lo);
    g.setColour (accent.withAlpha (slider.isEnabled() ? 0.9F : 0.4F));
    g.fillRoundedRectangle (fill, kTrackWidth * 0.5F);

    // Cap: a raised bar across the groove with an accent index line.
    const float capLong = static_cast<float> (kThumbRadius) * 2.0F - 2.0F;
    const float capShort = std::min (span - 2.0F, 40.0F);
    const auto cap = vertical ? juce::Rectangle<float> (capShort, capLong).withCentre ({ across, sliderPos })
                              : juce::Rectangle<float> (capLong, capShort).withCentre ({ sliderPos, across });
    g.setColour (shadow());
    g.fillRoundedRectangle (cap.translated (0.0F, 2.0F), 4.0F);
    g.setGradientFill (juce::ColourGradient (slider.isMouseOverOrDragging() ? capTop().brighter (0.15F) : capTop(),
                                             cap.getX(), cap.getY(), capBottom(), cap.getX(), cap.getBottom(), false));
    g.fillRoundedRectangle (cap, 4.0F);
    g.setColour (highlight());
    g.drawRoundedRectangle (cap.reduced (0.5F), 4.0F, 1.0F);
    g.setColour (accent);
    vertical ? g.fillRect (cap.withSizeKeepingCentre (cap.getWidth() - 8.0F, 2.0F))
             : g.fillRect (cap.withSizeKeepingCentre (2.0F, cap.getHeight() - 8.0F));
}

juce::Label* GlassLookAndFeel::createSliderTextBox (juce::Slider& slider)
{
    auto* label = juce::LookAndFeel_V4::createSliderTextBox (slider);
    label->setFont (theme::font (theme::typeSmall()));
    label->setMinimumHorizontalScale (0.7F);
    return label;
}

void GlassLookAndFeel::drawButtonBackground (juce::Graphics& g, juce::Button& button, const juce::Colour& backgroundColour,
                                             bool isHighlighted, bool isDown)
{
    using namespace theme;
    const auto bounds = button.getLocalBounds().toFloat().reduced (0.5F);
    const bool on = button.getToggleState();
    auto fill = on ? button.findColour (juce::TextButton::buttonOnColourId) : backgroundColour;
    if (! on && (isHighlighted || isDown))
        fill = isDown ? surfaceHover().withMultipliedAlpha (1.4F) : surfaceHover();
    else if (on && isDown)
        fill = fill.darker (0.2F);
    if (! button.isEnabled())
        fill = fill.withMultipliedAlpha (0.5F);

    juce::Path shape;
    shape.addRoundedRectangle (bounds.getX(), bounds.getY(), bounds.getWidth(), bounds.getHeight(), controlRadius(), controlRadius(),
                               ! (button.isConnectedOnLeft() || button.isConnectedOnTop()),
                               ! (button.isConnectedOnRight() || button.isConnectedOnTop()),
                               ! (button.isConnectedOnLeft() || button.isConnectedOnBottom()),
                               ! (button.isConnectedOnRight() || button.isConnectedOnBottom()));
    if (on)
    {
        g.setColour (fill.withAlpha (0.3F));
        g.fillRoundedRectangle (bounds.expanded (1.5F), controlRadius() + 1.5F);   // soft glow
    }
    g.setColour (fill);
    g.fillPath (shape);
    g.setColour (on ? fill.brighter (0.3F) : border());
    g.strokePath (shape, juce::PathStrokeType (1.0F));
}

juce::Font GlassLookAndFeel::getTextButtonFont (juce::TextButton&, int buttonHeight)
{
    return theme::font (std::min (theme::typeBody(), static_cast<float> (buttonHeight) * 0.5F), true);
}

void GlassLookAndFeel::drawDocumentWindowTitleBar (juce::DocumentWindow& window, juce::Graphics& g, int w, int h,
                                                   int titleSpaceX, int titleSpaceW, const juce::Image*, bool)
{
    if (w * h == 0)
        return;
    // A see-through window has a transparent background; keep the bar itself frosted.
    const auto alpha = std::max (0.85F, window.findColour (juce::ResizableWindow::backgroundColourId).getFloatAlpha());
    g.setColour (theme::backdropTop().withMultipliedAlpha (alpha));
    g.fillAll();
    g.setColour (theme::border());
    g.fillRect (0, h - 1, w, 1);
    g.setColour (window.isActiveWindow() ? theme::textSecondary() : theme::textMuted());
    g.setFont (theme::font (static_cast<float> (h) * 0.5F, true));
    g.drawText (window.getName(), titleSpaceX, 0, titleSpaceW, h, juce::Justification::centred, true);
}

} // namespace openmixer::ui
