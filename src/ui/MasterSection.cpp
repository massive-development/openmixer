#include "MasterSection.h"

#include "Theme.h"

namespace openmixer::ui
{

namespace { constexpr double kMaxMaster = 1.5; constexpr int kMaxFaderWidth = 40; }

MasterSection::MasterSection (MixerBinding b) : binding (b)
{
    styleCaption (title, "Master");
    styleCaption (faderCaption, "Level");
    styleCaption (meterCaption, "L   R");
    fader.setRange (0.0, kMaxMaster, 0.001);
    fader.setValue (1.0, juce::dontSendNotification);
    fader.setDoubleClickReturnValue (true, 1.0);
    fader.onValueChange = [this] { binding.send ("master", fader.getValue()); };
    for (juce::Component* c : { static_cast<juce::Component*> (&title), static_cast<juce::Component*> (&faderCaption),
                                static_cast<juce::Component*> (&meterCaption), static_cast<juce::Component*> (&fader),
                                static_cast<juce::Component*> (&meter) })
        addAndMakeVisible (c);
    applyTheme();
}

void MasterSection::applyTheme()
{
    title.setFont (theme::font (theme::typeLabel(), true));
    title.setColour (juce::Label::textColourId, theme::textPrimary());
    for (auto* l : { &faderCaption, &meterCaption })
    {
        l->setFont (theme::font (theme::typeCaption()));
        l->setColour (juce::Label::textColourId, theme::textSecondary());
    }
    fader.setColour (juce::Slider::trackColourId, theme::master());
}

void MasterSection::refresh (float dtSeconds)
{
    auto& m = binding.mixer();
    if (! fader.isMouseButtonDown())
        fader.setValue (m.getMasterGain(), juce::dontSendNotification);
    meter.update (m.getMasterPeakLeft(), m.getMasterPeakRight(), dtSeconds);
}

void MasterSection::paint (juce::Graphics& g)
{
    theme::paintGlassCard (g, getLocalBounds().toFloat().reduced (0.5F), theme::master());
}

void MasterSection::resized()
{
    const int gap = theme::innerGap() / 2;
    auto area = getLocalBounds().reduced (theme::pad() - 4, theme::pad() - 2);
    title.setBounds (area.removeFromTop (22));
    area.removeFromTop (gap);

    auto meterColumn = area.removeFromRight (theme::mixerMeterWidth() * 2 + 2);
    meterCaption.setBounds (meterColumn.removeFromTop (kCaptionHeight).expanded (6, 0));
    meter.setBounds (meterColumn);
    area.removeFromRight (gap);
    faderCaption.setBounds (area.removeFromTop (kCaptionHeight));
    fader.setBounds (area.withSizeKeepingCentre (std::min (area.getWidth(), kMaxFaderWidth), area.getHeight()));
}

} // namespace openmixer::ui
