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

juce::String MidiMapper::toJson() const
{
    const std::lock_guard<std::recursive_mutex> guard (lock);
    return map.toJson();
}

juce::Result MidiMapper::saveToFile (const juce::File& file) const
{
    if (const auto r = file.getParentDirectory().createDirectory(); r.failed())
        return r;
    // Replace atomically so a crash mid-save keeps the old map.
    return file.replaceWithText (toJson()) ? juce::Result::ok() : juce::Result::fail ("Could not write " + file.getFullPathName());
}

void MidiMapper::install (MidiMap&& newMap)
{
    const std::lock_guard<std::recursive_mutex> guard (lock);
    map = std::move (newMap);
    values.clear();
    halves14.clear();
    rebuildIndex();
}

void MidiMapper::rebuildIndex()
{
    index.clear();
    for (size_t i = 0; i < map.mappings.size(); ++i)
    {
        const auto& m = map.mappings[i];
        index[makeKey (m.kind, m.channel, m.number)].push_back (static_cast<int> (i));
        if (m.mode == MapMode::Absolute14)
            index[makeKey (m.kind, m.channel, m.lsbNumber)].push_back (static_cast<int> (i));
        values.emplace (m.target, m.initial);
    }
    bump();
}

double MidiMapper::getValue (const juce::String& id) const
{
    const std::lock_guard<std::recursive_mutex> guard (lock);
    const auto it = values.find (id);
    return it != values.end() ? it->second : 0.0;
}

std::vector<Mapping> MidiMapper::getBindings() const
{
    const std::lock_guard<std::recursive_mutex> guard (lock);
    return map.mappings;
}

juce::Result MidiMapper::addBinding (const Mapping& m)
{
    if (const auto r = m.validate(); r.failed())
        return r;
    const std::lock_guard<std::recursive_mutex> guard (lock);
    map.mappings.push_back (m);
    rebuildIndex();
    return juce::Result::ok();
}

juce::Result MidiMapper::replaceBinding (size_t i, const Mapping& m)
{
    if (const auto r = m.validate(); r.failed())
        return r;
    const std::lock_guard<std::recursive_mutex> guard (lock);
    if (i >= map.mappings.size())
        return juce::Result::fail ("no binding at index " + juce::String (static_cast<int> (i)));
    map.mappings[i] = m;
    rebuildIndex();
    return juce::Result::ok();
}

bool MidiMapper::removeBinding (size_t i)
{
    const std::lock_guard<std::recursive_mutex> guard (lock);
    if (i >= map.mappings.size())
        return false;
    map.mappings.erase (map.mappings.begin() + static_cast<std::ptrdiff_t> (i));
    rebuildIndex();
    return true;
}

int MidiMapper::removeBindingsFor (const juce::String& targetId)
{
    const std::lock_guard<std::recursive_mutex> guard (lock);
    const auto before = map.mappings.size();
    map.mappings.erase (std::remove_if (map.mappings.begin(), map.mappings.end(),
                                        [&] (const Mapping& m) { return m.target == targetId; }), map.mappings.end());
    rebuildIndex();
    return static_cast<int> (before - map.mappings.size());
}

std::vector<OutputRule> MidiMapper::getOutputs() const
{
    const std::lock_guard<std::recursive_mutex> guard (lock);
    return map.outputs;
}

void MidiMapper::setOutputs (std::vector<OutputRule> rules)
{
    const std::lock_guard<std::recursive_mutex> guard (lock);
    map.outputs = std::move (rules);
    bump();
}

void MidiMapper::setOnLearned (std::function<void (const Mapping&)> callback)
{
    const std::lock_guard<std::recursive_mutex> guard (lock);
    onLearned = std::move (callback);
}

// A learned control replaces whatever the target or the control was bound to before.
void MidiMapper::bind (const Mapping& learned)
{
    auto& list = map.mappings;
    list.erase (std::remove_if (list.begin(), list.end(), [&] (const Mapping& m)
                                { return m.target == learned.target
                                      || (m.kind == learned.kind && m.channel == learned.channel && m.number == learned.number); }),
                list.end());
    list.push_back (learned);
    values[learned.target] = learned.initial;
    rebuildIndex();
}

bool MidiMapper::captureLearned (const MidiEvent& e)
{
    const auto learned = learn.capture (e);
    if (! learned)
        return false;
    std::function<void (const Mapping&)> callback;
    {
        const std::lock_guard<std::recursive_mutex> guard (lock);
        bind (*learned);
        callback = onLearned;
    }
    if (callback)
        callback (*learned);
    return true;
}

int MidiMapper::handleMessage (const juce::MidiMessage& m)
{
    MidiEvent e;
    return MidiEvent::fromMessage (m, e) ? handleEvent (e) : 0;
}

int MidiMapper::handleEvent (const MidiEvent& e)
{
    if (learn.isArmed() && captureLearned (e))
        return 0;

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
        case MapMode::Absolute14:
        {
            const bool wide = e.kind == MidiEvent::Kind::PitchBend || m.mode == MapMode::Absolute14;
            const double norm = static_cast<double> (e.value) / (wide ? 16383.0 : 127.0);
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

// MSB and LSB arrive as separate CCs; keep the halves and report the combined 14-bit value.
void MidiMapper::combine14 (const Mapping& m, MidiEvent& e)
{
    auto& bits = halves14[makeKey (MidiEvent::Kind::CC, e.channel, m.number)];
    bits = e.number == m.number ? ((e.value << 7) | (bits & 0x7F)) : ((bits & ~0x7F) | e.value);
    e.value = bits;
}

bool MidiMapper::apply (const Mapping& m, const MidiEvent& incoming)
{
    MidiEvent e = incoming;
    if (m.mode == MapMode::Absolute14)
        combine14 (m, e);

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
