// Settings tab: driver, output device, sample rate, buffer size, master/headphone output pairs and live
// latency / xrun / CPU numbers. Device changes go through an AudioSettingsModel (live), then the new
// device state and pair choice are stored in the preferences. Message thread only.
#pragma once

#include "AudioSettingsModel.h"
#include "SettingsPage.h"
#include "engine/LatencyMonitor.h"

namespace openmixer::ui
{

class AudioSettingsPage : public SettingsPage, private juce::Timer
{
public:
    using StatsProvider = std::function<LatencyStats()>;

    AudioSettingsPage (Preferences& prefs, AudioSettingsModel& model, StatsProvider stats);
    ~AudioSettingsPage() override = default;

    void reflect() override;
    void refreshStats();   // the timer calls this; tests call it directly

    juce::ComboBox& getTypeBox() noexcept { return typeBox; }
    juce::ComboBox& getDeviceBox() noexcept { return deviceBox; }
    juce::ComboBox& getRateBox() noexcept { return rateBox; }
    juce::ComboBox& getBufferBox() noexcept { return bufferBox; }
    juce::ComboBox& getMasterBox() noexcept { return masterBox; }
    juce::ComboBox& getCueBox() noexcept { return cueBox; }
    const juce::Label& getStatsLabel() const noexcept { return statsLabel; }
    const juce::String& getError() const noexcept { return errorText; }

private:
    void fillDevices();
    void fillPairs();
    void commit (const juce::String& error);
    void pairChanged();
    void timerCallback() override { refreshStats(); }

    Preferences& prefs;
    AudioSettingsModel& model;
    StatsProvider stats;
    juce::ComboBox typeBox, deviceBox, rateBox, bufferBox, masterBox, cueBox;
    juce::Label statsLabel;
    juce::String errorText;
    juce::Array<double> rates;
    juce::Array<int> buffers;
    bool filling = false;   // programmatic combo updates must not re-enter the handlers
};

} // namespace openmixer::ui
