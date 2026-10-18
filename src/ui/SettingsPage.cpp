#include "SettingsPage.h"

#include "Theme.h"

namespace openmixer::ui
{

void SettingsPage::addRow (const juce::String& caption, juce::Component& control, int rows)
{
    auto label = std::make_unique<juce::Label> (juce::String(), caption);
    addAndMakeVisible (*label);
    addAndMakeVisible (control);
    items.push_back ({ std::move (label), &control, rows, false });
    applyTheme();
}

void SettingsPage::addWide (juce::Component& control, int rows)
{
    addAndMakeVisible (control);
    items.push_back ({ nullptr, &control, rows, false });
}

juce::Label& SettingsPage::addNote (const juce::String& text)
{
    auto label = std::make_unique<juce::Label> (juce::String(), text);
    auto& ref = *label;
    addAndMakeVisible (ref);
    items.push_back ({ nullptr, label.get(), 1, true });
    items.back().caption = std::move (label);   // owned through `caption`; control points at it
    applyTheme();
    return ref;
}

void SettingsPage::applyTheme()
{
    for (auto& row : items)
        if (row.caption != nullptr)
        {
            row.caption->setFont (theme::font (row.isNote ? theme::typeCaption() : theme::typeBody()));
            row.caption->setColour (juce::Label::textColourId, row.isNote ? theme::textMuted() : theme::textSecondary());
            row.caption->setMinimumHorizontalScale (1.0F);
        }
}

void SettingsPage::resized()
{
    auto area = getLocalBounds().reduced (theme::pad());
    const int rowH = theme::settingsRowHeight(), gap = theme::innerGap(), labelW = theme::settingsLabelWidth();
    for (auto& row : items)
    {
        const int h = rowH * row.rows + gap * (row.rows - 1);
        auto strip = area.removeFromTop (h);
        area.removeFromTop (gap);
        if (row.isNote)
            row.control->setBounds (strip);
        else if (row.caption != nullptr)
        {
            row.caption->setBounds (strip.removeFromLeft (labelW).withHeight (rowH));
            row.control->setBounds (strip.withHeight (row.rows == 1 ? rowH : h));
        }
        else
            row.control->setBounds (strip);
    }
}

} // namespace openmixer::ui
