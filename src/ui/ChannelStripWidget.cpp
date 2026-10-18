#include "ChannelStripWidget.h"

#include "Theme.h"
#include "ValueFormat.h"

namespace openmixer::ui
{

namespace
{
    constexpr int kTitleHeight = 22, kAssignGroup = 1001, kMaxFaderWidth = 40;
    constexpr const char* kEqIds[ChannelStripWidget::kBands] { "eq.low", "eq.mid", "eq.high" };
    constexpr const char* kEqText[ChannelStripWidget::kBands] { "Lo", "Mid", "Hi" };
    constexpr const char* kAssignText[3] { "Mid", "L", "R" };

    juce::String compactDb (double db) { return juce::String (juce::roundToInt (db)) + " dB"; }
    juce::String compactGain (double linear) { return linear <= 0.0 ? juce::String ("-inf") : compactDb (20.0 * std::log10 (linear)); }
}

ChannelStripWidget::ChannelStripWidget (MixerBinding b, int ch) : binding (b), channel (ch), accent (theme::accentFor (ch))
{
    build();
    applyTheme();
    refresh (0.0F);
}

void ChannelStripWidget::build()
{
    styleCaption (title, "Channel " + juce::String (channel + 1));
    title.setJustificationType (juce::Justification::centredLeft);
    styleCaption (faderCaption, "Level");
    addAndMakeVisible (title);
    addAndMakeVisible (faderCaption);

    trim = std::make_unique<LabeledKnob> (LabeledKnob::Spec { "Trim", 0.0, 4.0, 1.0, compactGain });
    trim->onChange = [this] (double v) { send ("trim", v); };
    filter = std::make_unique<LabeledKnob> (LabeledKnob::Spec { "Filter", -1.0, 1.0, 0.0, format::filter });
    filter->onChange = [this] (double v) { send ("filter", v); };
    addAndMakeVisible (*trim);
    addAndMakeVisible (*filter);

    buildEq();
    buildButtons();
    fader.setRange (0.0, 1.0, 0.001);
    fader.setValue (1.0, juce::dontSendNotification);
    fader.setDoubleClickReturnValue (true, 1.0);
    fader.onValueChange = [this] { send ("fader", fader.getValue()); };
    addAndMakeVisible (fader);
    addAndMakeVisible (meter);
}

void ChannelStripWidget::buildEq()
{
    for (int i = 0; i < kBands; ++i)
    {
        const auto band = static_cast<size_t> (i);
        eq[band] = std::make_unique<LabeledKnob> (LabeledKnob::Spec { kEqText[i], Mixer::kMinEqDb, Mixer::kMaxEqDb, 0.0, compactDb });
        eq[band]->onChange = [this, i] (double v) { send (kEqIds[i], v); };
        addAndMakeVisible (*eq[band]);

        kill[band].setButtonText ("Kill");
        kill[band].setTooltip ("Cut this EQ band completely");
        kill[band].onClick = [this, i] { pressKill (i); };
        addAndMakeVisible (kill[band]);
    }
}

void ChannelStripWidget::buildButtons()
{
    pfl.setTooltip ("Pre-fader listen: send this channel to the headphone cue");
    pfl.onClick = [this] { send ("pfl", binding.mixer().getPfl (channel) ? 0.0 : 1.0); refresh (0.0F); };
    fxButton.setTooltip ("Show this channel's effects rack");
    fxButton.onClick = [this] { if (onFxRequested) onFxRequested (channel); };
    addAndMakeVisible (pfl);
    addAndMakeVisible (fxButton);

    for (size_t i = 0; i < assign.size(); ++i)
    {
        assign[i].setButtonText (kAssignText[i]);
        assign[i].setTooltip ("Crossfader assignment: L, Mid (bypass, always on) or R");
        assign[i].setRadioGroupId (kAssignGroup, juce::dontSendNotification);
        assign[i].onClick = [this, i] { send ("xfader", static_cast<double> (i)); refresh (0.0F); };
        addAndMakeVisible (assign[i]);
    }
    // Visual order L | Thru | R, joined into one segmented control.
    assign[1].setConnectedEdges (juce::Button::ConnectedOnRight);
    assign[0].setConnectedEdges (juce::Button::ConnectedOnLeft | juce::Button::ConnectedOnRight);
    assign[2].setConnectedEdges (juce::Button::ConnectedOnLeft);
}

bool ChannelStripWidget::isKilled (int band) const
{
    return binding.mixer().getEq (channel, band) <= Mixer::kMinEqDb + 0.01F;
}

// Kill drops the band to the engine floor and remembers where it was; release goes back there.
void ChannelStripWidget::pressKill (int band)
{
    const auto idx = static_cast<size_t> (band);
    if (isKilled (band))
        send (kEqIds[band], restoreDb[idx]);
    else
    {
        restoreDb[idx] = binding.mixer().getEq (channel, band);
        send (kEqIds[band], Mixer::kMinEqDb);
    }
    refresh (0.0F);
}

void ChannelStripWidget::applyTheme()
{
    accent = theme::accentFor (channel);
    title.setFont (theme::font (theme::typeLabel(), true));
    title.setColour (juce::Label::textColourId, theme::textPrimary());
    faderCaption.setFont (theme::font (theme::typeCaption()));
    faderCaption.setColour (juce::Label::textColourId, theme::textSecondary());
    fader.setColour (juce::Slider::trackColourId, accent);
    for (auto* b : { &pfl, &fxButton, &assign[0], &assign[1], &assign[2] })
        b->setColour (juce::TextButton::buttonOnColourId, accent);
    for (auto& b : kill)
        b.setColour (juce::TextButton::buttonOnColourId, theme::meterHigh());
    trim->setAccent (accent);
    filter->setAccent (accent);
    for (auto& k : eq)
        k->setAccent (accent);
}

void ChannelStripWidget::refresh (float dtSeconds)
{
    auto& m = binding.mixer();
    trim->setValueQuiet (m.getTrim (channel));
    filter->setValueQuiet (m.getFilter (channel));
    for (int i = 0; i < kBands; ++i)
    {
        eq[static_cast<size_t> (i)]->setValueQuiet (m.getEq (channel, i));
        kill[static_cast<size_t> (i)].setToggleState (isKilled (i), juce::dontSendNotification);
    }
    if (! fader.isMouseButtonDown())
        fader.setValue (m.getFader (channel), juce::dontSendNotification);
    pfl.setToggleState (m.getPfl (channel), juce::dontSendNotification);
    assign[static_cast<size_t> (m.getCrossfaderAssign (channel))].setToggleState (true, juce::dontSendNotification);
    meter.update (m.getChannelPeakLeft (channel), m.getChannelPeakRight (channel), dtSeconds);
}

void ChannelStripWidget::paint (juce::Graphics& g)
{
    const auto bounds = getLocalBounds().toFloat().reduced (0.5F);
    theme::paintGlassCard (g, bounds, accent);
    const auto t = title.getBounds().toFloat();
    g.setColour (accent);
    g.fillEllipse (juce::Rectangle<float> (8.0F, 8.0F).withCentre ({ t.getX() - 8.0F, t.getCentreY() }));
}

void ChannelStripWidget::layoutKnobs (juce::Rectangle<int>& area)
{
    const int h = theme::mixerKnobHeight();
    auto top = area.removeFromTop (h);
    trim->setBounds (top.removeFromLeft (top.getWidth() / 2));
    filter->setBounds (top);

    auto row = area.removeFromTop (h);
    const int cell = row.getWidth() / kBands;
    auto killRow = area.removeFromTop (theme::mixerButtonHeight() - 4);
    for (int i = 0; i < kBands; ++i)   // displayed Hi | Mid | Lo, as on a hardware mixer
    {
        const auto band = static_cast<size_t> (kBands - 1 - i);
        eq[band]->setBounds (row.removeFromLeft (cell));
        kill[band].setBounds (killRow.removeFromLeft (cell).reduced (1, 0));
    }
}

void ChannelStripWidget::layoutButtons (juce::Rectangle<int>& area)
{
    const int h = theme::mixerButtonHeight(), gap = theme::innerGap() / 2;
    auto row = area.removeFromTop (h);
    pfl.setBounds (row.removeFromLeft (row.getWidth() / 2).reduced (1, 0));
    fxButton.setBounds (row.reduced (1, 0));
    area.removeFromTop (gap);
    auto segments = area.removeFromTop (h);
    const int third = segments.getWidth() / 3;
    for (size_t i : { size_t { 1 }, size_t { 0 }, size_t { 2 } })   // L, Thru, R
        assign[i].setBounds (i == 2 ? segments : segments.removeFromLeft (third));
}

void ChannelStripWidget::resized()
{
    const int gap = theme::innerGap() / 2;
    auto area = getLocalBounds().reduced (theme::pad() - 6, theme::pad() - 2);
    title.setBounds (area.removeFromTop (kTitleHeight).withTrimmedLeft (14));
    area.removeFromTop (gap);
    layoutKnobs (area);
    area.removeFromTop (gap);
    layoutButtons (area);
    area.removeFromTop (gap);

    faderCaption.setBounds (area.removeFromTop (kCaptionHeight));
    const int meterWidth = theme::mixerMeterWidth() * 2 + 2;
    auto group = area.withSizeKeepingCentre (std::min (area.getWidth(), kMaxFaderWidth + gap + meterWidth), area.getHeight());
    meter.setBounds (group.removeFromRight (meterWidth));
    group.removeFromRight (gap);
    fader.setBounds (group);
}

} // namespace openmixer::ui
