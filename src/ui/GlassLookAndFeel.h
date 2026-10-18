// Glass LookAndFeel (Todo 25): ring knobs, capped faders, pill buttons and the form widgets
// (combo box, popup menu, text editor, toggle, tabs, scroll bar, table header, tooltip), all drawn
// from theme tokens. It follows theme switches: colour ids are re-applied and every window repainted.
// Per-control accents come from colour ids: Slider::rotarySliderFillColourId / trackColourId and
// TextButton::buttonOnColourId. Message thread only.
#pragma once

#include "Theme.h"

#include <juce_gui_basics/juce_gui_basics.h>

namespace openmixer::ui
{

class GlassLookAndFeel : public juce::LookAndFeel_V4, private theme::ThemeListener
{
public:
    GlassLookAndFeel();
    ~GlassLookAndFeel() override;

    void drawRotarySlider (juce::Graphics&, int x, int y, int width, int height, float sliderPos,
                           float rotaryStartAngle, float rotaryEndAngle, juce::Slider&) override;
    void drawLinearSlider (juce::Graphics&, int x, int y, int width, int height, float sliderPos,
                           float minSliderPos, float maxSliderPos, juce::Slider::SliderStyle, juce::Slider&) override;
    int getSliderThumbRadius (juce::Slider&) override;
    juce::Label* createSliderTextBox (juce::Slider&) override;

    void drawButtonBackground (juce::Graphics&, juce::Button&, const juce::Colour& backgroundColour,
                               bool isHighlighted, bool isDown) override;
    juce::Font getTextButtonFont (juce::TextButton&, int buttonHeight) override;

    void drawDocumentWindowTitleBar (juce::DocumentWindow&, juce::Graphics&, int w, int h, int titleSpaceX,
                                     int titleSpaceW, const juce::Image* icon, bool drawTitleTextOnLeft) override;

    // Form widgets (GlassWidgets.cpp).
    void drawComboBox (juce::Graphics&, int width, int height, bool isButtonDown, int buttonX, int buttonY, int buttonW,
                       int buttonH, juce::ComboBox&) override;
    juce::Font getComboBoxFont (juce::ComboBox&) override;
    void positionComboBoxText (juce::ComboBox&, juce::Label&) override;

    void drawPopupMenuBackground (juce::Graphics&, int width, int height) override;
    void drawPopupMenuItem (juce::Graphics&, const juce::Rectangle<int>& area, bool isSeparator, bool isActive, bool isHighlighted,
                            bool isTicked, bool hasSubMenu, const juce::String& text, const juce::String& shortcutKeyText,
                            const juce::Drawable* icon, const juce::Colour* textColour) override;
    juce::Font getPopupMenuFont() override;

    void fillTextEditorBackground (juce::Graphics&, int width, int height, juce::TextEditor&) override;
    void drawTextEditorOutline (juce::Graphics&, int width, int height, juce::TextEditor&) override;

    void drawToggleButton (juce::Graphics&, juce::ToggleButton&, bool isHighlighted, bool isDown) override;

    void drawTabButton (juce::TabBarButton&, juce::Graphics&, bool isMouseOver, bool isMouseDown) override;
    void drawTabbedButtonBarBackground (juce::TabbedButtonBar&, juce::Graphics&) override;
    void drawTabAreaBehindFrontButton (juce::TabbedButtonBar&, juce::Graphics&, int width, int height) override;

    void drawScrollbar (juce::Graphics&, juce::ScrollBar&, int x, int y, int width, int height, bool isScrollbarVertical,
                        int thumbStartPosition, int thumbSize, bool isMouseOver, bool isMouseDown) override;

    void drawTableHeaderColumn (juce::Graphics&, juce::TableHeaderComponent&, const juce::String& columnName, int columnId,
                                int width, int height, bool isMouseOver, bool isMouseDown, int columnFlags) override;

    void drawTooltip (juce::Graphics&, const juce::String& text, int width, int height) override;

    // Where a control's value arc/fill starts: its reset value when it has one, else its minimum.
    static double originOf (const juce::Slider& s);

private:
    void themeChanged() override;
    void applyTheme();
    void applyControlColours();
    void applyContainerColours();
};

} // namespace openmixer::ui
