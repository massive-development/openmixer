// Deck header: title/artist, effective BPM, key + Camelot code, time elapsed and remaining.
// Metadata comes from the library/analysis via setTrack(); the clock is read from the deck. Message thread only.
#pragma once

#include "DeckBinding.h"

#include <juce_gui_basics/juce_gui_basics.h>

namespace openmixer::ui
{

struct TrackInfo
{
    juce::String title, artist, key, camelot;
    double bpm = 0.0;   // 0 = unknown; a beat grid on the deck takes precedence
};

class TrackHeader : public juce::Component
{
public:
    explicit TrackHeader (DeckBinding binding);

    void setTrack (const TrackInfo& info);
    void refresh();   // frame tick: clock + BPM

    static juce::String formatTime (double seconds, bool remaining);   // "1:05.3" / "-0:12.0"
    static juce::String formatBpm (double bpm);                         // "128.0" or "--"
    static juce::String formatKey (const TrackInfo& info);              // "Am  8A"

    const juce::Label& getElapsedLabel() const noexcept { return elapsed; }
    const juce::Label& getRemainingLabel() const noexcept { return remaining; }
    const juce::Label& getBpmLabel() const noexcept { return bpmLabel; }
    const juce::Label& getKeyLabel() const noexcept { return keyLabel; }
    const juce::Label& getTitleLabel() const noexcept { return title; }
    const juce::Label& getArtistLabel() const noexcept { return artist; }

    void resized() override;
    void lookAndFeelChanged() override { applyTheme(); }

private:
    void applyTheme();
    double effectiveBpm() const;
    static void setIfChanged (juce::Label& label, const juce::String& text);

    DeckBinding deckRef;
    TrackInfo info;
    juce::Label title, artist, bpmLabel, keyLabel, elapsed, remaining;
};

} // namespace openmixer::ui
