#include "MidiMapper.h"

#include <algorithm>

namespace openmixer
{

bool MidiEvent::fromMessage (const juce::MidiMessage& m, MidiEvent& out) noexcept
{
    if (m.isController())
    {
        out = { Kind::CC, m.getChannel(), m.getControllerNumber(), m.getControllerValue(), false };
        return true;
    }
    if (m.isNoteOn())
    {
        out = { Kind::Note, m.getChannel(), m.getNoteNumber(), m.getVelocity(), true };
        return true;
    }
    if (m.isNoteOff())
    {
        out = { Kind::Note, m.getChannel(), m.getNoteNumber(), 0, false };
        return true;
    }
    if (m.isPitchWheel())
    {
        out = { Kind::PitchBend, m.getChannel(), 0, m.getPitchWheelValue(), false };
        return true;
    }
    return false;
}

//==============================================================================
juce::Result MidiMapper::loadFromJson (const juce::String& json)
{
    auto parsed = juce::JSON::parse (json);
    auto* root = parsed.getDynamicObject();
    if (root == nullptr)
        return juce::Result::fail ("Map is not a JSON object");

    auto list = root->getProperty ("mappings");
    if (! list.isArray())
        return juce::Result::fail ("Map has no \"mappings\" array");

    std::vector<Mapping> loaded;
    for (int i = 0; i < list.size(); ++i)
    {
        auto* o = list[i].getDynamicObject();
        auto where = "mapping " + juce::String (i) + ": ";
        if (o == nullptr)
            return juce::Result::fail (where + "not an object");

        Mapping m;
        const auto type = o->getProperty ("type").toString().toLowerCase();
        if (type == "cc")             m.kind = MidiEvent::Kind::CC;
        else if (type == "note")      m.kind = MidiEvent::Kind::Note;
        else if (type == "pitchbend") m.kind = MidiEvent::Kind::PitchBend;
        else return juce::Result::fail (where + "unknown type \"" + type + "\"");

        m.target = o->getProperty ("target").toString();
        if (m.target.isEmpty())
            return juce::Result::fail (where + "missing target");

        m.channel = (int) o->getProperty ("channel");
        if (m.channel < 0 || m.channel > 16)
            return juce::Result::fail (where + "channel must be 1-16");

        m.number = (int) o->getProperty ("number");
        if (m.kind != MidiEvent::Kind::PitchBend && (m.number < 0 || m.number > 127))
            return juce::Result::fail (where + "number must be 0-127");
        if (m.kind == MidiEvent::Kind::PitchBend)
            m.number = 0;

        m.mode = m.kind == MidiEvent::Kind::Note ? MapMode::Button : MapMode::Absolute;
        if (o->hasProperty ("mode"))
        {
            const auto mode = o->getProperty ("mode").toString().toLowerCase();
            if (mode == "absolute")        m.mode = MapMode::Absolute;
            else if (mode == "relative")   m.mode = MapMode::Relative;
            else if (mode == "relative2c") m.mode = MapMode::RelativeTwosComplement;
            else if (mode == "button")     m.mode = MapMode::Button;
            else if (mode == "toggle")     m.mode = MapMode::Toggle;
            else return juce::Result::fail (where + "unknown mode \"" + mode + "\"");
        }

        if (o->hasProperty ("min")) m.min = (double) o->getProperty ("min");
        if (o->hasProperty ("max")) m.max = (double) o->getProperty ("max");
        if (m.min == m.max)
            return juce::Result::fail (where + "min and max must differ");
        m.step = o->hasProperty ("step") ? (double) o->getProperty ("step") : (m.max - m.min) / 127.0;
        m.initial = o->hasProperty ("initial") ? (double) o->getProperty ("initial") : m.min;
        m.invert = (bool) o->getProperty ("invert");
        loaded.push_back (m);
    }

    // Commit only when the whole map is valid.
    name = root->getProperty ("name").toString();
    mappings = std::move (loaded);
    index.clear();
    values.clear();
    for (size_t i = 0; i < mappings.size(); ++i)
    {
        const auto& m = mappings[i];
        index[makeKey (m.kind, m.channel, m.number)].push_back ((int) i);
        values.emplace (m.target, m.initial);
    }
    return juce::Result::ok();
}

juce::Result MidiMapper::loadFromFile (const juce::File& file)
{
    if (! file.existsAsFile())
        return juce::Result::fail ("Map file not found: " + file.getFullPathName());
    return loadFromJson (file.loadFileAsString());
}

double MidiMapper::getValue (const juce::String& id) const
{
    auto it = values.find (id);
    return it != values.end() ? it->second : 0.0;
}

int MidiMapper::handleMessage (const juce::MidiMessage& m)
{
    MidiEvent e;
    return MidiEvent::fromMessage (m, e) ? handleEvent (e) : 0;
}

int MidiMapper::handleEvent (const MidiEvent& e)
{
    int fired = 0;
    for (int ch : { e.channel, 0 })
    {
        auto it = index.find (makeKey (e.kind, ch, e.number));
        if (it == index.end())
            continue;
        for (int i : it->second)
            apply (mappings[(size_t) i], e, fired);
    }
    return fired;
}

void MidiMapper::apply (const Mapping& m, const MidiEvent& e, int& fired)
{
    auto& current = values[m.target];
    const bool isBend = e.kind == MidiEvent::Kind::PitchBend;
    const bool pressed = e.kind == MidiEvent::Kind::Note ? e.noteOn : e.value > 0;
    double result = current;

    switch (m.mode)
    {
        case MapMode::Absolute:
        {
            double norm = (double) e.value / (isBend ? 16383.0 : 127.0);
            if (m.invert) norm = 1.0 - norm;
            result = m.min + norm * (m.max - m.min);
            break;
        }
        case MapMode::Relative:
        case MapMode::RelativeTwosComplement:
        {
            int delta = m.mode == MapMode::Relative ? e.value - 64
                                                    : (e.value < 64 ? e.value : e.value - 128);
            if (m.invert) delta = -delta;
            result = current + delta * m.step;
            break;
        }
        case MapMode::Button:
            result = pressed ? m.max : m.min;
            if (m.invert) result = pressed ? m.min : m.max;
            break;
        case MapMode::Toggle:
            if (! pressed)
                return;
            result = (current == m.max) ? m.min : m.max;
            break;
    }

    const double lo = std::min (m.min, m.max), hi = std::max (m.min, m.max);
    result = std::clamp (result, lo, hi);
    current = result;
    if (target != nullptr)
        target->setParameter (m.target, result);
    ++fired;
}

} // namespace openmixer
