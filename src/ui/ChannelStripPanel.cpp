#include "ChannelStripPanel.h"

#include "Theme.h"
#include "ValueFormat.h"

namespace openmixer::ui
{

namespace
{
    constexpr int kKnobCellHeight = 80, kMinKnobCell = 50, kKnobTextWidth = 60, kKnobTextHeight = 16;
    constexpr int kTitleHeight = 22, kButtonHeight = 28, kMeterColumnWidth = 24, kMaxFaderWidth = 64;
    constexpr int kMaxContentWidth = 400;   // wide windows centre the controls instead of spreading them
    constexpr int kAssignGroup = 1001, kKnobCount = 5;

    int knobColumns (int width) { return juce::jlimit (1, kKnobCount, width / kMinKnobCell); }
}

ChannelStripPanel::ChannelStripPanel (Mixer& m, int ch) : mixer (m), channel (ch), accent (theme::accentFor (ch))
{
    const auto prefix = "ch" + juce::String (channel) + ".";
    const char* eqIds[3] { "eq.low", "eq.mid", "eq.high" };
    const char* eqText[3] { "Lo", "Mid", "Hi" };
    for (size_t i = 0; i < eq.size(); ++i)
        addKnob (eq[i], eqCaptions[i], eqText[i], prefix + eqIds[i], Mixer::kMinEqDb, Mixer::kMaxEqDb, 0.0, format::decibels);
    addKnob (filter, filterCaption, "Filter", prefix + "filter", -1.0, 1.0, 0.0, format::filter);

    styleCaption (titleLabel, "Channel " + juce::String (channel + 1));
    titleLabel.setJustificationType (juce::Justification::centredLeft);
    styleCaption (faderCaption, "Fader");
    styleCaption (sideCaption, "X-fader");
    styleCaption (meterCaption, "Lvl");
    for (auto* l : { &titleLabel, &faderCaption, &sideCaption, &meterCaption })
        addAndMakeVisible (*l);

    fader.setSliderStyle (juce::Slider::LinearVertical);
    fader.setTextBoxStyle (juce::Slider::NoTextBox, false, 0, 0);
    fader.setRange (0.0, 1.0, 0.001);
    fader.setValue (1.0, juce::dontSendNotification);
    fader.onValueChange = [this, prefix] { mixer.setParameter (prefix + "fader", fader.getValue()); };
    addAndMakeVisible (fader);
    addKnob (trim, trimCaption, "Trim", prefix + "trim", 0.0, 4.0, 1.0, format::gainAsDecibels);   // after fader: keeps child order stable

    pfl.setClickingTogglesState (true);
    pfl.setTooltip ("Pre-fader listen: send this channel to the headphone cue");
    pfl.onClick = [this] { mixer.setPfl (channel, pfl.getToggleState()); };
    addAndMakeVisible (pfl);

    addAssignButtons();
    addAndMakeVisible (meter);
    applyTheme();
}

// Theme-dependent bits live here so a theme switch can re-apply them.
void ChannelStripPanel::applyTheme()
{
    accent = theme::accentFor (channel);
    titleLabel.setFont (theme::font (theme::typeLabel(), true));
    titleLabel.setColour (juce::Label::textColourId, theme::textPrimary());
    fader.setColour (juce::Slider::trackColourId, accent);
    pfl.setColour (juce::TextButton::buttonOnColourId, accent);
    for (auto& b : assign)
        b.setColour (juce::TextButton::buttonOnColourId, accent);
    for (auto* k : { &trim, &filter, &eq[0], &eq[1], &eq[2] })
        k->setColour (juce::Slider::rotarySliderFillColourId, accent);
    for (auto* l : { &faderCaption, &sideCaption, &meterCaption, &trimCaption, &filterCaption, &eqCaptions[0], &eqCaptions[1], &eqCaptions[2] })
    {
        l->setFont (theme::font (theme::typeCaption()));
        l->setColour (juce::Label::textColourId, theme::textSecondary());
    }
}

void ChannelStripPanel::lookAndFeelChanged() { applyTheme(); }

void ChannelStripPanel::addKnob (juce::Slider& s, juce::Label& caption, const juce::String& text, const juce::String& id,
                                 double lo, double hi, double initial, std::function<juce::String (double)> format)
{
    s.setSliderStyle (juce::Slider::RotaryHorizontalVerticalDrag);
    s.setTextBoxStyle (juce::Slider::TextBoxBelow, true, kKnobTextWidth, kKnobTextHeight);
    s.textFromValueFunction = std::move (format);
    s.setRange (lo, hi, 0.01);
    s.setValue (initial, juce::dontSendNotification);
    s.setDoubleClickReturnValue (true, initial);
    s.onValueChange = [this, &s, id] { mixer.setParameter (id, s.getValue()); };
    styleCaption (caption, text);
    addAndMakeVisible (s);
    addAndMakeVisible (caption);
}

void ChannelStripPanel::addAssignButtons()
{
    const char* text[3] { "Thru", "L", "R" };
    for (size_t i = 0; i < assign.size(); ++i)
    {
        auto& b = assign[i];
        b.setButtonText (text[i]);
        b.setClickingTogglesState (true);
        b.setRadioGroupId (kAssignGroup, juce::dontSendNotification);
        b.onClick = [this, i]
        {
            if (assign[i].getToggleState())
                mixer.setCrossfaderAssign (channel, static_cast<CrossfaderAssign> (i));
        };
        addAndMakeVisible (b);
    }
    assign[static_cast<size_t> (CrossfaderAssign::Thru)].setToggleState (true, juce::dontSendNotification);
    // Visual order L | Thru | R, joined into one segmented control.
    assign[1].setConnectedEdges (juce::Button::ConnectedOnRight);
    assign[0].setConnectedEdges (juce::Button::ConnectedOnLeft | juce::Button::ConnectedOnRight);
    assign[2].setConnectedEdges (juce::Button::ConnectedOnLeft);
}

void ChannelStripPanel::refresh()
{
    meter.setLevel (mixer.getChannelPeak (channel));
}

void ChannelStripPanel::paint (juce::Graphics& g)
{
    const auto bounds = getLocalBounds().toFloat().reduced (0.5F);
    theme::paintGlassCard (g, bounds, accent);

    const auto title = titleLabel.getBounds().toFloat();
    g.setColour (accent);
    g.fillEllipse (juce::Rectangle<float> (8.0F, 8.0F).withCentre ({ title.getX() - 10.0F, title.getCentreY() }));
    g.setColour (theme::border());
    g.fillRect (juce::Rectangle<float> (bounds.getX() + theme::pad(), title.getBottom() + 3.0F, bounds.getWidth() - 2.0F * theme::pad(), 1.0F));
}

// Every knob gets the same cell size, so knobs line up in one row (or a regular grid when narrow).
void ChannelStripPanel::layoutKnobs (juce::Rectangle<int> area)
{
    struct Knob { juce::Label& caption; juce::Slider& slider; };
    const std::array<Knob, kKnobCount> knobs { { { trimCaption, trim }, { eqCaptions[2], eq[2] }, { eqCaptions[1], eq[1] },
                                        { eqCaptions[0], eq[0] }, { filterCaption, filter } } };
    const int columns = knobColumns (area.getWidth());
    const int cellWidth = area.getWidth() / columns;
    for (int i = 0; i < kKnobCount; ++i)
    {
        const auto cell = juce::Rectangle<int> (area.getX() + (i % columns) * cellWidth, area.getY() + (i / columns) * kKnobCellHeight,
                                                cellWidth, kKnobCellHeight).reduced (2, 0);
        placeCaptioned (cell, knobs[static_cast<size_t> (i)].caption, knobs[static_cast<size_t> (i)].slider);
    }
}

void ChannelStripPanel::layoutFaderZone (juce::Rectangle<int> area)
{
    auto controls = area.removeFromLeft (juce::jlimit (72, 132, area.getWidth() * 2 / 5));
    area.removeFromLeft (theme::innerGap());
    pfl.setBounds (controls.removeFromTop (kButtonHeight));
    controls.removeFromTop (theme::innerGap());
    sideCaption.setBounds (controls.removeFromTop (kCaptionHeight));
    auto segments = controls.removeFromTop (kButtonHeight);
    const int third = segments.getWidth() / 3;
    for (size_t i : { size_t { 1 }, size_t { 0 }, size_t { 2 } })   // L, Thru, R
        assign[i].setBounds (i == 2 ? segments : segments.removeFromLeft (third));

    // Fader and meter stay side by side as one group, centred in what is left.
    auto group = area.withSizeKeepingCentre (std::min (area.getWidth(), kMaxFaderWidth + 4 + kMeterColumnWidth), area.getHeight());
    auto meterColumn = group.removeFromRight (kMeterColumnWidth);
    meterCaption.setBounds (meterColumn.removeFromTop (kCaptionHeight));
    meter.setBounds (meterColumn.withSizeKeepingCentre (12, meterColumn.getHeight()));
    group.removeFromRight (4);
    placeCaptioned (group, faderCaption, fader);
}

void ChannelStripPanel::resized()
{
    auto area = getLocalBounds().reduced (theme::pad());
    titleLabel.setBounds (area.removeFromTop (kTitleHeight).withTrimmedLeft (18));
    area.removeFromTop (theme::innerGap());
    area = area.withSizeKeepingCentre (std::min (area.getWidth(), kMaxContentWidth), area.getHeight());

    const int columns = knobColumns (area.getWidth());
    const int rows = (kKnobCount + columns - 1) / columns;
    layoutKnobs (area.removeFromTop (rows * kKnobCellHeight));
    area.removeFromTop (theme::innerGap());
    layoutFaderZone (area);
}

} // namespace openmixer::ui
