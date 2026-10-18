// Shared helpers for captioned controls: fixed caption strip above each control.
#pragma once

#include "Theme.h"

#include <juce_gui_basics/juce_gui_basics.h>

namespace openmixer::ui
{

constexpr int kCaptionHeight = 16;

// Caption text shrinks horizontally instead of eliding when the cell is narrow.
inline void styleCaption (juce::Label& label, const juce::String& text)
{
    label.setText (text, juce::dontSendNotification);
    label.setJustificationType (juce::Justification::centred);
    label.setFont (theme::font (theme::typeCaption()));
    label.setColour (juce::Label::textColourId, theme::textSecondary());
    label.setBorderSize ({ 0, 0, 0, 0 });
    label.setMinimumHorizontalScale (0.6F);
    label.setInterceptsMouseClicks (false, false);
}

inline void placeCaptioned (juce::Rectangle<int> cell, juce::Label& caption, juce::Component& control)
{
    caption.setBounds (cell.removeFromTop (kCaptionHeight));
    control.setBounds (cell);
}

} // namespace openmixer::ui
