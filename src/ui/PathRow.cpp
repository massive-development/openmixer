#include "PathRow.h"

#include "Theme.h"

namespace openmixer::ui
{

PathRow::PathRow (juce::String chooserTitle, juce::String filePattern)
    : title (std::move (chooserTitle)), pattern (std::move (filePattern))
{
    path.setMinimumHorizontalScale (1.0F);
    path.setJustificationType (juce::Justification::centredLeft);
    addAndMakeVisible (path);
    addAndMakeVisible (choose);
    addAndMakeVisible (reset);
    choose.onClick = [this] { openChooser(); };
    reset.onClick = [this] { useDefault(); };
    applyTheme();
}

void PathRow::applyTheme()
{
    path.setFont (theme::font (theme::typeBody()));
    path.setColour (juce::Label::textColourId, theme::textPrimary());
}

void PathRow::setPath (const juce::String& text, bool isDefault)
{
    path.setText (text, juce::dontSendNotification);
    path.setTooltip (text);
    reset.setEnabled (! isDefault);
}

void PathRow::openChooser()
{
    chooser = std::make_unique<juce::FileChooser> (title, juce::File(), pattern);
    const auto chooserFlags = pattern.isEmpty() ? juce::FileBrowserComponent::openMode | juce::FileBrowserComponent::canSelectDirectories
                                         : juce::FileBrowserComponent::openMode | juce::FileBrowserComponent::canSelectFiles;
    chooser->launchAsync (chooserFlags, [this] (const juce::FileChooser& fc)
    {
        if (const auto file = fc.getResult(); file != juce::File())
            pick (file);
    });
}

void PathRow::resized()
{
    auto area = getLocalBounds();
    const int w = theme::settingsButtonWidth();
    reset.setBounds (area.removeFromRight (w));
    area.removeFromRight (theme::innerGap());
    choose.setBounds (area.removeFromRight (w));
    area.removeFromRight (theme::innerGap());
    path.setBounds (area);
}

} // namespace openmixer::ui
