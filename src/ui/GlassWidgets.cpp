// Glass styling for form widgets: combo box, popup menu, text editor, toggle, tabs, scroll bar,
// table header and tooltip. Colours come from colour ids (set in applyTheme) or live tokens.
#include "GlassLookAndFeel.h"

#include "Theme.h"

namespace openmixer::ui
{

namespace
{
    constexpr float kSwitchWidth = 34.0F, kSwitchHeight = 18.0F, kThumbInset = 2.0F;
    constexpr int kMenuItemInset = 4, kTextInset = 8, kTabUnderline = 2;

    void strokeBox (juce::Graphics& g, juce::Rectangle<float> r, juce::Colour fill, juce::Colour outline, float thickness)
    {
        g.setColour (fill);
        g.fillRoundedRectangle (r, theme::controlRadius());
        g.setColour (outline);
        g.drawRoundedRectangle (r.reduced (thickness * 0.5F), theme::controlRadius(), thickness);
    }

    juce::Path chevron (juce::Rectangle<float> area)
    {
        juce::Path p;
        const auto c = area.getCentre();
        p.startNewSubPath (c.x - 4.0F, c.y - 2.0F);
        p.lineTo (c.x, c.y + 2.0F);
        p.lineTo (c.x + 4.0F, c.y - 2.0F);
        return p;
    }

    // The thin strip of a tab bar facing its content, where the underline and rule go.
    juce::Rectangle<int> contentEdge (juce::TabbedButtonBar::Orientation o, juce::Rectangle<int> r, int thickness)
    {
        switch (o)
        {
            case juce::TabbedButtonBar::TabsAtTop: return r.removeFromBottom (thickness);
            case juce::TabbedButtonBar::TabsAtBottom: return r.removeFromTop (thickness);
            case juce::TabbedButtonBar::TabsAtLeft: return r.removeFromRight (thickness);
            case juce::TabbedButtonBar::TabsAtRight: return r.removeFromLeft (thickness);
        }
        return {};
    }
}

//==============================================================================
// Combo box

void GlassLookAndFeel::drawComboBox (juce::Graphics& g, int width, int height, bool, int buttonX, int buttonY, int buttonW,
                                     int buttonH, juce::ComboBox& box)
{
    const auto bounds = juce::Rectangle<int> (0, 0, width, height).toFloat();
    const bool focused = box.hasKeyboardFocus (true);
    strokeBox (g, bounds, box.findColour (juce::ComboBox::backgroundColourId),
               box.findColour (focused ? juce::ComboBox::focusedOutlineColourId : juce::ComboBox::outlineColourId),
               focused ? 1.5F : 1.0F);
    g.setColour (box.findColour (juce::ComboBox::arrowColourId).withMultipliedAlpha (box.isEnabled() ? 1.0F : 0.4F));
    g.strokePath (chevron (juce::Rectangle<int> (buttonX, buttonY, buttonW, buttonH).toFloat()), juce::PathStrokeType (1.6F));
}

juce::Font GlassLookAndFeel::getComboBoxFont (juce::ComboBox&) { return theme::font (theme::typeBody()); }

void GlassLookAndFeel::positionComboBoxText (juce::ComboBox& box, juce::Label& label)
{
    label.setBounds (kTextInset, 1, box.getWidth() - box.getHeight() - kTextInset, box.getHeight() - 2);
    label.setFont (getComboBoxFont (box));
}

//==============================================================================
// Popup menu

void GlassLookAndFeel::drawPopupMenuBackground (juce::Graphics& g, int width, int height)
{
    const auto bounds = juce::Rectangle<int> (0, 0, width, height).toFloat();
    strokeBox (g, bounds, findColour (juce::PopupMenu::backgroundColourId), theme::border(), 1.0F);
}

juce::Font GlassLookAndFeel::getPopupMenuFont() { return theme::font (theme::typeBody()); }

void GlassLookAndFeel::drawPopupMenuItem (juce::Graphics& g, const juce::Rectangle<int>& area, bool isSeparator, bool isActive,
                                          bool isHighlighted, bool isTicked, bool hasSubMenu, const juce::String& text,
                                          const juce::String& shortcutKeyText, const juce::Drawable* icon, const juce::Colour* textColour)
{
    if (isSeparator)
    {
        g.setColour (theme::border());
        g.fillRect (area.reduced (kTextInset, 0).withSizeKeepingCentre (area.getWidth() - 2 * kTextInset, 1));
        return;
    }

    const bool lit = isHighlighted && isActive;
    if (lit)
    {
        g.setColour (findColour (juce::PopupMenu::highlightedBackgroundColourId));
        g.fillRoundedRectangle (area.reduced (kMenuItemInset, 1).toFloat(), theme::controlRadius() - 2.0F);
    }
    const auto ink = (textColour != nullptr ? *textColour : findColour (lit ? juce::PopupMenu::highlightedTextColourId
                                                                             : juce::PopupMenu::textColourId))
                         .withMultipliedAlpha (isActive ? 1.0F : 0.4F);
    g.setColour (ink);
    g.setFont (getPopupMenuFont());

    auto row = area.reduced (kTextInset + kMenuItemInset, 0);
    const auto gutter = row.removeFromLeft (16);   // tick / icon column
    if (icon != nullptr)
        icon->drawWithin (g, gutter.toFloat(), juce::RectanglePlacement::centred, 1.0F);
    else if (isTicked)
        g.fillEllipse (gutter.toFloat().withSizeKeepingCentre (6.0F, 6.0F));
    if (hasSubMenu)
        g.strokePath (chevron (row.removeFromRight (12).toFloat()).createPathWithRoundedCorners (1.0F),
                      juce::PathStrokeType (1.4F), juce::AffineTransform::rotation (-juce::MathConstants<float>::halfPi,
                                                                                    static_cast<float> (row.getRight()), static_cast<float> (row.getCentreY())));
    g.drawFittedText (text, row, juce::Justification::centredLeft, 1);
    if (shortcutKeyText.isNotEmpty())
    {
        g.setColour (ink.withMultipliedAlpha (0.6F));
        g.drawText (shortcutKeyText, row, juce::Justification::centredRight, false);
    }
}

//==============================================================================
// Text editor

void GlassLookAndFeel::fillTextEditorBackground (juce::Graphics& g, int width, int height, juce::TextEditor& editor)
{
    g.setColour (editor.findColour (juce::TextEditor::backgroundColourId));
    g.fillRoundedRectangle (juce::Rectangle<int> (0, 0, width, height).toFloat(), theme::controlRadius());
}

void GlassLookAndFeel::drawTextEditorOutline (juce::Graphics& g, int width, int height, juce::TextEditor& editor)
{
    const bool focused = editor.hasKeyboardFocus (true) && ! editor.isReadOnly();
    g.setColour (editor.findColour (focused ? juce::TextEditor::focusedOutlineColourId : juce::TextEditor::outlineColourId));
    const float thickness = focused ? 1.5F : 1.0F;
    g.drawRoundedRectangle (juce::Rectangle<int> (0, 0, width, height).toFloat().reduced (thickness * 0.5F),
                            theme::controlRadius(), thickness);
}

//==============================================================================
// Toggle button: a switch, label to its right

void GlassLookAndFeel::drawToggleButton (juce::Graphics& g, juce::ToggleButton& button, bool isHighlighted, bool isDown)
{
    const auto bounds = button.getLocalBounds().toFloat();
    const auto track = juce::Rectangle<float> (kSwitchWidth, kSwitchHeight).withCentre ({ bounds.getX() + kSwitchWidth * 0.5F + 2.0F, bounds.getCentreY() });
    const bool on = button.getToggleState();
    const float fade = button.isEnabled() ? 1.0F : 0.4F;

    auto fill = on ? button.findColour (juce::ToggleButton::tickColourId) : theme::well();
    if (isHighlighted || isDown)
        fill = fill.brighter (isDown ? 0.25F : 0.12F);
    g.setColour (fill.withMultipliedAlpha (fade));
    g.fillRoundedRectangle (track, kSwitchHeight * 0.5F);
    g.setColour (theme::border());
    g.drawRoundedRectangle (track.reduced (0.5F), kSwitchHeight * 0.5F, 1.0F);

    const float d = kSwitchHeight - 2.0F * kThumbInset;
    const float thumbX = on ? track.getRight() - kThumbInset - d : track.getX() + kThumbInset;
    g.setColour (theme::textPrimary().withMultipliedAlpha (fade));
    g.fillEllipse (thumbX, track.getY() + kThumbInset, d, d);

    g.setColour (button.findColour (juce::ToggleButton::textColourId).withMultipliedAlpha (fade));
    g.setFont (theme::font (theme::typeBody()));
    g.drawFittedText (button.getButtonText(), button.getLocalBounds().withTrimmedLeft (static_cast<int> (kSwitchWidth) + theme::innerGap() + 2),
                      juce::Justification::centredLeft, 1);
}

//==============================================================================
// Tabs: flat labels with an accent underline on the front tab

void GlassLookAndFeel::drawTabButton (juce::TabBarButton& button, juce::Graphics& g, bool isMouseOver, bool isMouseDown)
{
    const auto area = button.getActiveArea();
    const bool front = button.isFrontTab();
    if (isMouseOver || isMouseDown)
    {
        g.setColour (theme::surfaceHover());
        g.fillRoundedRectangle (area.reduced (2, 3).toFloat(), theme::controlRadius() - 2.0F);
    }
    if (front)
    {
        g.setColour (button.getTabbedButtonBar().findColour (juce::TabbedButtonBar::frontOutlineColourId));
        g.fillRect (contentEdge (button.getTabbedButtonBar().getOrientation(), area, kTabUnderline));
    }
    g.setColour ((front || isMouseOver ? theme::textPrimary() : theme::textSecondary()).withMultipliedAlpha (button.isEnabled() ? 1.0F : 0.4F));
    g.setFont (theme::font (theme::typeBody(), front));
    g.drawFittedText (button.getButtonText(), button.getTextArea(), juce::Justification::centred, 1);
}

void GlassLookAndFeel::drawTabbedButtonBarBackground (juce::TabbedButtonBar&, juce::Graphics&) {}

void GlassLookAndFeel::drawTabAreaBehindFrontButton (juce::TabbedButtonBar& bar, juce::Graphics& g, int width, int height)
{
    g.setColour (bar.findColour (juce::TabbedButtonBar::tabOutlineColourId));
    g.fillRect (contentEdge (bar.getOrientation(), { 0, 0, width, height }, 1));
}

//==============================================================================
// Scroll bar: a slim rounded thumb, no track

void GlassLookAndFeel::drawScrollbar (juce::Graphics& g, juce::ScrollBar& bar, int x, int y, int width, int height,
                                      bool isScrollbarVertical, int thumbStartPosition, int thumbSize, bool isMouseOver, bool isMouseDown)
{
    if (thumbSize <= 0)
        return;
    const auto track = juce::Rectangle<int> (x, y, width, height).toFloat();
    const auto thumb = isScrollbarVertical ? juce::Rectangle<float> (track.getX(), static_cast<float> (thumbStartPosition), track.getWidth(), static_cast<float> (thumbSize))
                                           : juce::Rectangle<float> (static_cast<float> (thumbStartPosition), track.getY(), static_cast<float> (thumbSize), track.getHeight());
    const auto slim = isScrollbarVertical ? thumb.reduced (3.0F, 1.0F) : thumb.reduced (1.0F, 3.0F);
    auto colour = bar.findColour (juce::ScrollBar::thumbColourId);
    if (isMouseOver || isMouseDown)
        colour = colour.interpolatedWith (theme::textPrimary(), isMouseDown ? 0.6F : 0.35F);
    g.setColour (colour);
    g.fillRoundedRectangle (slim, std::min (slim.getWidth(), slim.getHeight()) * 0.5F);
}

//==============================================================================
// Table header

void GlassLookAndFeel::drawTableHeaderColumn (juce::Graphics& g, juce::TableHeaderComponent& header, const juce::String& columnName,
                                              int, int width, int height, bool isMouseOver, bool isMouseDown, int columnFlags)
{
    if (isMouseOver || isMouseDown)
    {
        g.setColour (header.findColour (juce::TableHeaderComponent::highlightColourId).withMultipliedAlpha (isMouseDown ? 1.0F : 0.7F));
        g.fillRect (0, 0, width, height);
    }
    g.setColour (header.findColour (juce::TableHeaderComponent::outlineColourId));
    g.fillRect (width - 1, height / 4, 1, height / 2);   // column divider

    auto row = juce::Rectangle<int> (0, 0, width, height).reduced (kTextInset, 0);
    const bool forwards = (columnFlags & juce::TableHeaderComponent::sortedForwards) != 0;
    if (forwards || (columnFlags & juce::TableHeaderComponent::sortedBackwards) != 0)
    {
        auto arrow = row.removeFromRight (12).toFloat().withSizeKeepingCentre (8.0F, 5.0F);
        juce::Path p;
        p.addTriangle (arrow.getX(), forwards ? arrow.getBottom() : arrow.getY(), arrow.getRight(), forwards ? arrow.getBottom() : arrow.getY(),
                       arrow.getCentreX(), forwards ? arrow.getY() : arrow.getBottom());
        g.setColour (theme::accentFor (0));
        g.fillPath (p);
    }
    g.setColour (header.findColour (juce::TableHeaderComponent::textColourId));
    g.setFont (theme::font (theme::typeCaption(), true));
    g.drawFittedText (columnName, row, juce::Justification::centredLeft, 1);
}

//==============================================================================
// Tooltip

void GlassLookAndFeel::drawTooltip (juce::Graphics& g, const juce::String& text, int width, int height)
{
    const auto bounds = juce::Rectangle<int> (0, 0, width, height).toFloat();
    strokeBox (g, bounds, findColour (juce::TooltipWindow::backgroundColourId), findColour (juce::TooltipWindow::outlineColourId), 1.0F);
    g.setColour (findColour (juce::TooltipWindow::textColourId));
    g.setFont (theme::font (theme::typeCaption()));
    g.drawFittedText (text, bounds.toNearestInt().reduced (kTextInset, 4), juce::Justification::centredLeft, 6);
}

} // namespace openmixer::ui
