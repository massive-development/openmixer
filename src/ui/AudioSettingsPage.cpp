#include "AudioSettingsPage.h"

#include "Theme.h"

namespace openmixer::ui
{

namespace
{
    constexpr int kStatsRefreshMs = 500;
    constexpr int kNoneId = 1;   // cue box: "None"; pair p has id p + 2 (master box: p + 1)

    juce::String pairName (int pair) { return "Out " + juce::String (pair * 2 + 1) + "+" + juce::String (pair * 2 + 2); }
}

AudioSettingsPage::AudioSettingsPage (Preferences& p, AudioSettingsModel& m, StatsProvider s)
    : prefs (p), model (m), stats (std::move (s))
{
    addRow ("Driver", typeBox);
    addRow ("Output device", deviceBox);
    addRow ("Sample rate", rateBox);
    addRow ("Buffer size", bufferBox);
    addRow ("Master output", masterBox);
    addRow ("Headphone output", cueBox);
    addRow ("Latency and load", statsLabel, 2);
    addNote ("Master and headphone mixes go to the chosen stereo pairs of the output device.");

    statsLabel.setJustificationType (juce::Justification::topLeft);
    typeBox.onChange = [this] { if (! filling) commit (model.setDeviceType (typeBox.getText())); };
    deviceBox.onChange = [this] { if (! filling) commit (model.setOutputDevice (deviceBox.getText())); };
    rateBox.onChange = [this]
    {
        if (! filling && rateBox.getSelectedItemIndex() >= 0)
            commit (model.setSampleRate (rates[rateBox.getSelectedItemIndex()]));
    };
    bufferBox.onChange = [this]
    {
        if (! filling && bufferBox.getSelectedItemIndex() >= 0)
            commit (model.setBufferSize (buffers[bufferBox.getSelectedItemIndex()]));
    };
    masterBox.onChange = [this] { if (! filling) pairChanged(); };
    cueBox.onChange = [this] { if (! filling) pairChanged(); };

    reflect();
    startTimer (kStatsRefreshMs);
}

// Fills every box from the model and the stored pairs; raises no change.
void AudioSettingsPage::reflect()
{
    const juce::ScopedValueSetter<bool> guard (filling, true);
    typeBox.clear (juce::dontSendNotification);
    typeBox.addItemList (model.getDeviceTypes(), 1);
    typeBox.setText (model.getDeviceType(), juce::dontSendNotification);
    fillDevices();
    fillPairs();
    refreshStats();
}

void AudioSettingsPage::fillDevices()
{
    deviceBox.clear (juce::dontSendNotification);
    deviceBox.addItemList (model.getOutputDevices(), 1);
    deviceBox.setText (model.getOutputDevice(), juce::dontSendNotification);

    rates = model.getSampleRates();
    rateBox.clear (juce::dontSendNotification);
    for (int i = 0; i < rates.size(); ++i)
        rateBox.addItem (juce::String (juce::roundToInt (rates[i])) + " Hz", i + 1);
    rateBox.setSelectedItemIndex (rates.indexOf (model.getSampleRate()), juce::dontSendNotification);

    buffers = model.getBufferSizes();
    bufferBox.clear (juce::dontSendNotification);
    for (int i = 0; i < buffers.size(); ++i)
        bufferBox.addItem (juce::String (buffers[i]) + " samples", i + 1);
    bufferBox.setSelectedItemIndex (buffers.indexOf (model.getBufferSize()), juce::dontSendNotification);
}

void AudioSettingsPage::fillPairs()
{
    const int pairs = std::max (1, model.getNumOutputChannels() / 2);
    masterBox.clear (juce::dontSendNotification);
    cueBox.clear (juce::dontSendNotification);
    cueBox.addItem ("None", kNoneId);
    for (int p = 0; p < pairs; ++p)
    {
        masterBox.addItem (pairName (p), p + 1);
        cueBox.addItem (pairName (p), p + 2);
    }
    masterBox.setSelectedId (prefs.audio.masterPair + 1, juce::dontSendNotification);
    cueBox.setSelectedId (prefs.audio.cuePair + 2 <= pairs + 1 ? prefs.audio.cuePair + 2 : kNoneId, juce::dontSendNotification);
    if (masterBox.getSelectedId() == 0)
        masterBox.setSelectedId (1, juce::dontSendNotification);   // a stored pair this device lacks shows the first one
}

// A device change: re-read what the new device offers and store its state.
void AudioSettingsPage::commit (const juce::String& error)
{
    errorText = error;
    {
        const juce::ScopedValueSetter<bool> guard (filling, true);
        fillDevices();
        fillPairs();
    }
    prefs.audio.deviceState = model.getStateXml();
    refreshStats();
    notifyChanged();
}

void AudioSettingsPage::pairChanged()
{
    prefs.audio.masterPair = std::max (0, masterBox.getSelectedId() - 1);
    prefs.audio.cuePair = cueBox.getSelectedId() - 2;   // None (id 1) -> -1
    notifyChanged();
}

void AudioSettingsPage::refreshStats()
{
    if (! stats)
        return;
    const auto s = stats();
    if (s.sampleRate <= 0.0)
    {
        statsLabel.setText (errorText.isNotEmpty() ? errorText : "No audio device running", juce::dontSendNotification);
        return;
    }
    statsLabel.setText (juce::String::formatted ("Output %.1f ms, round trip %.1f ms at %d Hz / %d samples\n"
                                                 "CPU %.0f%% (peak %.2f ms of %.2f ms), xruns %llu",
                                                 s.outputLatencyMs, s.roundTripLatencyMs, juce::roundToInt (s.sampleRate), s.bufferSize,
                                                 s.cpuLoad * 100.0, s.maxCallbackUs / 1000.0, s.budgetUs / 1000.0,
                                                 static_cast<unsigned long long> (s.xrunCount)),
                        juce::dontSendNotification);
}

} // namespace openmixer::ui
