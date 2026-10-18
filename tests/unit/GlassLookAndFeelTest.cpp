// OpenMixer GlassLookAndFeelTest - every styled widget paints, in both built-in themes
#include <JuceHeader.h>
#include "ui/GlassLookAndFeel.h"
#include "ui/Theme.h"
#include "ui/ThemeRegistry.h"

#include <gtest/gtest.h>

#include <set>

using namespace openmixer::ui;

namespace
{
bool hasInk (const juce::Image& image)
{
    for (int y = 0; y < image.getHeight(); ++y)
        for (int x = 0; x < image.getWidth(); ++x)
            if (image.getPixelAt (x, y).getAlpha() > 0)
                return true;
    return false;
}

// Number of distinct pixel values: a styled widget draws more than a flat fill.
int distinctPixels (const juce::Image& image)
{
    std::set<juce::uint32> seen;
    for (int y = 0; y < image.getHeight(); ++y)
        for (int x = 0; x < image.getWidth(); ++x)
            seen.insert (image.getPixelAt (x, y).getARGB());
    return static_cast<int> (seen.size());
}

juce::Image render (juce::Component& c, int w, int h)
{
    c.setBounds (0, 0, w, h);
    return c.createComponentSnapshot (c.getLocalBounds());
}

struct GlassLookAndFeelTest : ::testing::TestWithParam<const char*>
{
    ThemeTokens saved = theme::current();
    GlassLookAndFeel lnf;
    ThemeRegistry registry;

    GlassLookAndFeelTest()
    {
        registry.loadBuiltIns();
        registry.select (GetParam());
    }
    ~GlassLookAndFeelTest() override { theme::setCurrent (saved); }

    void expectPaints (juce::Component& c, int w, int h, int minDistinct = 3)
    {
        c.setLookAndFeel (&lnf);
        const auto image = render (c, w, h);
        c.setLookAndFeel (nullptr);
        ASSERT_FALSE (image.isNull());
        EXPECT_TRUE (hasInk (image));
        EXPECT_GE (distinctPixels (image), minDistinct);
    }
};
}

TEST_P (GlassLookAndFeelTest, ComboBox)
{
    juce::ComboBox box;
    box.addItem ("Alpha", 1);
    box.addItem ("Beta", 2);
    box.setSelectedId (2, juce::dontSendNotification);
    expectPaints (box, 160, 28);
}

TEST_P (GlassLookAndFeelTest, ComboBoxDisabled)
{
    juce::ComboBox box;
    box.addItem ("Alpha", 1);
    box.setSelectedId (1, juce::dontSendNotification);
    box.setEnabled (false);
    expectPaints (box, 160, 28);
}

TEST_P (GlassLookAndFeelTest, TextEditor)
{
    juce::TextEditor editor;
    editor.setText ("Search crates", false);
    expectPaints (editor, 200, 28);
}

TEST_P (GlassLookAndFeelTest, ToggleButtonOnAndOff)
{
    juce::ToggleButton toggle ("Sync");
    toggle.setToggleState (false, juce::dontSendNotification);
    expectPaints (toggle, 120, 24);
    toggle.setToggleState (true, juce::dontSendNotification);
    expectPaints (toggle, 120, 24);
}

TEST_P (GlassLookAndFeelTest, TabbedButtonBar)
{
    juce::TabbedButtonBar bar (juce::TabbedButtonBar::TabsAtTop);
    bar.addTab ("Library", juce::Colours::transparentBlack, -1);
    bar.addTab ("Effects", juce::Colours::transparentBlack, -1);
    bar.setCurrentTabIndex (1, false);
    expectPaints (bar, 260, 32);
}

TEST_P (GlassLookAndFeelTest, TabbedButtonBarEveryOrientation)
{
    for (auto o : { juce::TabbedButtonBar::TabsAtBottom, juce::TabbedButtonBar::TabsAtLeft, juce::TabbedButtonBar::TabsAtRight })
    {
        juce::TabbedButtonBar bar (o);
        bar.addTab ("One", juce::Colours::transparentBlack, -1);
        expectPaints (bar, 100, 100);
    }
}

TEST_P (GlassLookAndFeelTest, ScrollBar)
{
    juce::ScrollBar bar (true);
    bar.setRangeLimits (0.0, 100.0);
    bar.setCurrentRange (10.0, 30.0);
    expectPaints (bar, 12, 120, 2);
}

TEST_P (GlassLookAndFeelTest, TableHeader)
{
    juce::TableHeaderComponent header;
    header.addColumn ("Title", 1, 120, 30, 400, juce::TableHeaderComponent::defaultFlags);
    header.addColumn ("BPM", 2, 80, 30, 400, juce::TableHeaderComponent::defaultFlags);
    header.setSortColumnId (2, true);
    expectPaints (header, 200, 28);
}

TEST_P (GlassLookAndFeelTest, Tooltip)
{
    juce::Image image (juce::Image::ARGB, 200, 32, true);
    juce::Graphics g (image);
    lnf.drawTooltip (g, "Pre-fader listen", 200, 32);
    EXPECT_TRUE (hasInk (image));
    EXPECT_GE (distinctPixels (image), 3);
}

TEST_P (GlassLookAndFeelTest, PopupMenuBackgroundAndItems)
{
    juce::Image image (juce::Image::ARGB, 220, 120, true);
    juce::Graphics g (image);
    lnf.drawPopupMenuBackground (g, 220, 120);
    lnf.drawPopupMenuItem (g, { 0, 0, 220, 24 }, false, true, false, true, false, "Normal ticked", "Ctrl+N", nullptr, nullptr);
    lnf.drawPopupMenuItem (g, { 0, 24, 220, 24 }, false, true, true, false, false, "Highlighted", {}, nullptr, nullptr);
    lnf.drawPopupMenuItem (g, { 0, 48, 220, 24 }, false, false, false, false, false, "Disabled", {}, nullptr, nullptr);
    lnf.drawPopupMenuItem (g, { 0, 72, 220, 8 }, true, true, false, false, false, {}, {}, nullptr, nullptr);
    lnf.drawPopupMenuItem (g, { 0, 80, 220, 24 }, false, true, false, false, true, "Submenu", {}, nullptr, nullptr);
    EXPECT_TRUE (hasInk (image));
    EXPECT_GE (distinctPixels (image), 5);
}

TEST_P (GlassLookAndFeelTest, PopupMenuItemsAreMeasuredWithThemeFont)
{
    int w = 0, h = 0;
    lnf.getIdealPopupMenuItemSize ("First", false, -1, w, h);
    EXPECT_GT (w, 0);
    EXPECT_GT (h, 0);
    EXPECT_FLOAT_EQ (lnf.getPopupMenuFont().getHeight(), theme::typeBody());
}

TEST_P (GlassLookAndFeelTest, ExistingWidgetsStillPaint)
{
    juce::Slider knob (juce::Slider::RotaryHorizontalVerticalDrag, juce::Slider::NoTextBox);
    knob.setRange (0.0, 1.0);
    knob.setValue (0.7);
    expectPaints (knob, 64, 64);
    juce::TextButton button ("Play");
    button.setToggleState (true, juce::dontSendNotification);
    expectPaints (button, 80, 28);
}

INSTANTIATE_TEST_SUITE_P (BuiltInThemes, GlassLookAndFeelTest, ::testing::Values ("openmixer.dark", "openmixer.daylight"));
