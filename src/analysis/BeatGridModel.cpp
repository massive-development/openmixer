#include "BeatGridModel.h"

#include <algorithm>
#include <cmath>

namespace openmixer::analysis
{
namespace
{
bool validBpm (double bpm) noexcept { return std::isfinite (bpm) && bpm > 1.0 && bpm < 1000.0; }

juce::var markerToVar (const TempoMarker& m)
{
    auto obj = std::make_unique<juce::DynamicObject>();
    obj->setProperty ("frame", m.frame);
    obj->setProperty ("beat", m.beat);
    obj->setProperty ("bpm", m.bpm);
    return juce::var (obj.release());
}

std::optional<TempoMarker> markerFromVar (const juce::var& v)
{
    if (! v.hasProperty ("frame") || ! v.hasProperty ("beat") || ! v.hasProperty ("bpm"))
        return std::nullopt;
    TempoMarker m { static_cast<double> (v["frame"]), static_cast<double> (v["beat"]), static_cast<double> (v["bpm"]) };
    if (! validBpm (m.bpm) || ! std::isfinite (m.frame) || ! std::isfinite (m.beat))
        return std::nullopt;
    return m;
}
} // namespace

BeatGridModel BeatGridModel::constant (double bpm, double rate, double downbeatFrame, int barBeats)
{
    BeatGridModel grid;
    grid.sampleRate = rate;
    grid.beatsPerBar = std::max (1, barBeats);
    if (validBpm (bpm) && rate > 0.0)
        grid.markers.push_back ({ downbeatFrame, 0.0, bpm });
    return grid;
}

const TempoMarker& BeatGridModel::segmentForFrame (double frame) const noexcept
{
    auto it = std::upper_bound (markers.begin(), markers.end(), frame,
                                [] (double f, const TempoMarker& m) { return f < m.frame; });
    return it == markers.begin() ? markers.front() : *std::prev (it);
}

const TempoMarker& BeatGridModel::segmentForBeat (double beat) const noexcept
{
    auto it = std::upper_bound (markers.begin(), markers.end(), beat,
                                [] (double b, const TempoMarker& m) { return b < m.beat; });
    return it == markers.begin() ? markers.front() : *std::prev (it);
}

double BeatGridModel::bpmAtFrame (double frame) const noexcept
{
    return markers.empty() ? 0.0 : segmentForFrame (frame).bpm;
}

double BeatGridModel::beatAtFrame (double frame) const noexcept
{
    if (! isValid())
        return 0.0;
    const auto& m = segmentForFrame (frame);
    return m.beat + (frame - m.frame) / framesPerBeat (m);
}

double BeatGridModel::frameAtBeat (double beat) const noexcept
{
    if (! isValid())
        return 0.0;
    const auto& m = segmentForBeat (beat);
    return m.frame + (beat - m.beat) * framesPerBeat (m);
}

void BeatGridModel::setBpm (double bpm)
{
    if (! isValid() || ! validBpm (bpm))
        return;
    markers.resize (1);
    markers.front().bpm = bpm;
    manual = true;
}

void BeatGridModel::shift (double frames)
{
    for (auto& m : markers)
        m.frame += frames;
    manual = true;
}

void BeatGridModel::setDownbeatNear (double frame)
{
    if (! isValid())
        return;
    const auto nearest = std::round (beatAtFrame (frame));
    const auto offset = nearest - static_cast<double> (beatsPerBar) * std::floor (nearest / beatsPerBar);
    for (auto& m : markers)
        m.beat -= offset;
    manual = true;
}

void BeatGridModel::insertTempoChange (double frame, double bpm)
{
    if (! isValid() || ! validBpm (bpm))
        return;
    auto it = std::lower_bound (markers.begin(), markers.end(), frame,
                                [] (const TempoMarker& m, double f) { return m.frame < f; });
    if (it != markers.end() && it->frame == frame)
        it->bpm = bpm;
    else
        markers.insert (it, { frame, beatAtFrame (frame), bpm });

    for (size_t i = 1; i < markers.size(); ++i)
        markers[i].beat = markers[i - 1].beat + (markers[i].frame - markers[i - 1].frame) / framesPerBeat (markers[i - 1]);
    manual = true;
}

void BeatGridModel::setBeatsPerBar (int beats)
{
    beatsPerBar = std::max (1, beats);
    manual = true;
}

juce::var BeatGridModel::toVar() const
{
    juce::Array<juce::var> list;
    for (const auto& m : markers)
        list.add (markerToVar (m));

    auto obj = std::make_unique<juce::DynamicObject>();
    obj->setProperty ("version", kFormatVersion);
    obj->setProperty ("sampleRate", sampleRate);
    obj->setProperty ("beatsPerBar", beatsPerBar);
    obj->setProperty ("manual", manual);
    obj->setProperty ("markers", list);
    return juce::var (obj.release());
}

std::optional<BeatGridModel> BeatGridModel::fromVar (const juce::var& v)
{
    const auto* list = v["markers"].getArray();
    if (static_cast<int> (v["version"]) != kFormatVersion || list == nullptr || list->isEmpty())
        return std::nullopt;

    BeatGridModel grid;
    grid.sampleRate = static_cast<double> (v["sampleRate"]);
    grid.beatsPerBar = std::max (1, static_cast<int> (v["beatsPerBar"]));
    grid.manual = static_cast<bool> (v["manual"]);
    for (const auto& item : *list)
    {
        auto m = markerFromVar (item);
        if (! m || (! grid.markers.empty() && m->frame <= grid.markers.back().frame))
            return std::nullopt;
        grid.markers.push_back (*m);
    }
    return grid.sampleRate > 0.0 ? std::optional<BeatGridModel> (grid) : std::nullopt;
}

} // namespace openmixer::analysis
