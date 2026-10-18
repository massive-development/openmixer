#include "MidiMapper.h"

#include <algorithm>

namespace openmixer
{

juce::Result MidiMapper::loadFromJson (const juce::String& json)
{
    MidiMap parsed;
    if (const auto r = MidiMap::parse (json, parsed); r.failed())
        return r;
    install (std::move (parsed));
    return juce::Result::ok();
}

juce::Result MidiMapper::loadFromFile (const juce::File& file)
{
    if (! file.existsAsFile())
        return juce::Result::fail ("Map file not found: " + file.getFullPathName());
    return loadFromJson (file.loadFileAsString());
}

void MidiMapper::install (MidiMap&& newMap)
{
    const std::lock_guard<std::recursive_mutex> guard (lock);
    map = std::move (newMap);
    index.clear();
    values.clear();
    for (size_t i = 0; i < map.mappings.size(); ++i)
    {
        const auto& m = map.mappings[i];
        index[makeKey (m.kind, m.channel, m.number)].push_back (static_cast<int> (i));
        values.emplace (m.target, m.initial);
    }
}

double MidiMapper::getValue (const juce::String& id) const
{
    const std::lock_guard<std::recursive_mutex> guard (lock);
    const auto it = values.find (id);
    return it != values.end() ? it->second : 0.0;
}

int MidiMapper::handleMessage (const juce::MidiMessage& m)
{
    MidiEvent e;
    return MidiEvent::fromMessage (m, e) ? handleEvent (e) : 0;
}

int MidiMapper::handleEvent (const MidiEvent& e)
{
    const std::lock_guard<std::recursive_mutex> guard (lock);
    int fired = 0;
    for (const int channel : { e.channel, 0 })
    {
        const auto it = index.find (makeKey (e.kind, channel, e.number));
        if (it == index.end())
            continue;
        for (const int i : it->second)
            fired += apply (map.mappings[static_cast<size_t> (i)], e) ? 1 : 0;
    }
    return fired;
}

double MidiMapper::compute (const Mapping& m, const MidiEvent& e, double current, bool& changed) const noexcept
{
    const bool pressed = e.kind == MidiEvent::Kind::Note ? e.noteOn : e.value > 0;
    changed = true;
    switch (m.mode)
    {
        case MapMode::Absolute:
        {
            const double norm = static_cast<double> (e.value) / (e.kind == MidiEvent::Kind::PitchBend ? 16383.0 : 127.0);
            return m.min + (m.invert ? 1.0 - norm : norm) * (m.max - m.min);
        }
        case MapMode::Relative:
        case MapMode::RelativeTwosComplement:
        {
            const int raw = m.mode == MapMode::Relative ? e.value - 64 : (e.value < 64 ? e.value : e.value - 128);
            return current + (m.invert ? -raw : raw) * m.step;
        }
        case MapMode::Button:
            return pressed != m.invert ? m.max : m.min;
        case MapMode::Toggle:
            changed = pressed;
            return current == m.max ? m.min : m.max;
    }
    return current;
}

bool MidiMapper::apply (const Mapping& m, const MidiEvent& e)
{
    auto& current = values[m.target];
    bool changed = false;
    const double raw = compute (m, e, current, changed);
    if (! changed)
        return false;

    current = std::clamp (raw, std::min (m.min, m.max), std::max (m.min, m.max));
    if (target != nullptr)
        target->setParameter (m.target, current);
    return true;
}

} // namespace openmixer
