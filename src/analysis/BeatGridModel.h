// Editable beat grid: tempo markers, each starting a constant-tempo segment. Positions are source
// frames at the track's own sample rate; beat 0 (and every multiple of beatsPerBar) is a downbeat.
// Edits mark the grid manual so re-analysis can leave user corrections alone. Value type; not
// thread-safe (copy it, or hand an AnalysedBeatGrid snapshot to the engine).
#pragma once

#include <juce_core/juce_core.h>

#include <optional>
#include <vector>

namespace openmixer::analysis
{

struct TempoMarker
{
    double frame = 0.0;
    double beat = 0.0;
    double bpm = 120.0;
};

class BeatGridModel
{
public:
    static constexpr int kFormatVersion = 1;

    BeatGridModel() = default;
    static BeatGridModel constant (double bpm, double sampleRate, double downbeatFrame, int beatsPerBar = 4);

    bool isValid() const noexcept { return ! markers.empty() && sampleRate > 0.0; }
    bool isManual() const noexcept { return manual; }
    double getSampleRate() const noexcept { return sampleRate; }
    int getBeatsPerBar() const noexcept { return beatsPerBar; }
    const std::vector<TempoMarker>& getMarkers() const noexcept { return markers; }

    double getBpm() const noexcept { return markers.empty() ? 0.0 : markers.front().bpm; }
    double bpmAtFrame (double frame) const noexcept;
    double beatAtFrame (double frame) const noexcept;
    double frameAtBeat (double beat) const noexcept;

    // Manual corrections.
    void setBpm (double bpm);                         // keeps the first marker, drops tempo changes
    void shift (double frames);                       // moves the whole grid
    void setDownbeatNear (double frame);              // nearest beat becomes a downbeat
    void insertTempoChange (double frame, double bpm);
    void setBeatsPerBar (int beats);
    void setManual (bool isManualGrid) noexcept { manual = isManualGrid; }

    juce::var toVar() const;
    static std::optional<BeatGridModel> fromVar (const juce::var& v);

private:
    const TempoMarker& segmentForFrame (double frame) const noexcept;
    const TempoMarker& segmentForBeat (double beat) const noexcept;
    double framesPerBeat (const TempoMarker& m) const noexcept { return sampleRate * 60.0 / m.bpm; }

    std::vector<TempoMarker> markers;
    double sampleRate = 0.0;
    int beatsPerBar = 4;
    bool manual = false;
};

} // namespace openmixer::analysis
