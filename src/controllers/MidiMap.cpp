#include "MidiMap.h"

namespace openmixer
{

namespace
{
    juce::Result parseKind (const juce::DynamicObject& o, Mapping& m)
    {
        const auto type = o.getProperty ("type").toString().toLowerCase();
        if (type == "cc")             m.kind = MidiEvent::Kind::CC;
        else if (type == "note")      m.kind = MidiEvent::Kind::Note;
        else if (type == "pitchbend") m.kind = MidiEvent::Kind::PitchBend;
        else return juce::Result::fail ("unknown type \"" + type + "\"");
        return juce::Result::ok();
    }

    juce::Result parseAddress (const juce::DynamicObject& o, Mapping& m)
    {
        m.target = o.getProperty ("target").toString();
        if (m.target.isEmpty())
            return juce::Result::fail ("missing target");

        m.channel = static_cast<int> (o.getProperty ("channel"));
        if (m.channel < 0 || m.channel > 16)
            return juce::Result::fail ("channel must be 1-16");

        m.number = m.kind == MidiEvent::Kind::PitchBend ? 0 : static_cast<int> (o.getProperty ("number"));
        if (m.number < 0 || m.number > 127)
            return juce::Result::fail ("number must be 0-127");
        return juce::Result::ok();
    }

    juce::Result parseMode (const juce::DynamicObject& o, Mapping& m)
    {
        m.mode = m.kind == MidiEvent::Kind::Note ? MapMode::Button : MapMode::Absolute;
        if (! o.hasProperty ("mode"))
            return juce::Result::ok();

        const auto mode = o.getProperty ("mode").toString().toLowerCase();
        if (mode == "absolute")        m.mode = MapMode::Absolute;
        else if (mode == "relative")   m.mode = MapMode::Relative;
        else if (mode == "relative2c") m.mode = MapMode::RelativeTwosComplement;
        else if (mode == "button")     m.mode = MapMode::Button;
        else if (mode == "toggle")     m.mode = MapMode::Toggle;
        else return juce::Result::fail ("unknown mode \"" + mode + "\"");
        return juce::Result::ok();
    }

    juce::Result parseRange (const juce::DynamicObject& o, Mapping& m)
    {
        if (o.hasProperty ("min")) m.min = static_cast<double> (o.getProperty ("min"));
        if (o.hasProperty ("max")) m.max = static_cast<double> (o.getProperty ("max"));
        if (m.min == m.max)
            return juce::Result::fail ("min and max must differ");

        m.step = o.hasProperty ("step") ? static_cast<double> (o.getProperty ("step")) : (m.max - m.min) / 127.0;
        m.initial = o.hasProperty ("initial") ? static_cast<double> (o.getProperty ("initial")) : m.min;
        m.invert = static_cast<bool> (o.getProperty ("invert"));
        return juce::Result::ok();
    }

    juce::Result parseMapping (const juce::var& v, Mapping& m)
    {
        const auto* o = v.getDynamicObject();
        if (o == nullptr)
            return juce::Result::fail ("not an object");

        for (auto step : { &parseKind, &parseAddress, &parseMode, &parseRange })
            if (const auto r = step (*o, m); r.failed())
                return r;
        return juce::Result::ok();
    }
}

juce::Result MidiMap::parse (const juce::String& json, MidiMap& out)
{
    const auto parsed = juce::JSON::parse (json);
    const auto* root = parsed.getDynamicObject();
    if (root == nullptr)
        return juce::Result::fail ("Map is not a JSON object");

    const auto list = root->getProperty ("mappings");
    if (! list.isArray())
        return juce::Result::fail ("Map has no \"mappings\" array");

    MidiMap result;
    result.name = root->getProperty ("name").toString();
    for (int i = 0; i < list.size(); ++i)
    {
        Mapping m;
        if (const auto r = parseMapping (list[i], m); r.failed())
            return juce::Result::fail ("mapping " + juce::String (i) + ": " + r.getErrorMessage());
        result.mappings.push_back (std::move (m));
    }
    out = std::move (result);
    return juce::Result::ok();
}

} // namespace openmixer
