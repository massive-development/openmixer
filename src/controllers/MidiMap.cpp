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
        else if (mode == "absolute14") m.mode = MapMode::Absolute14;
        else return juce::Result::fail ("unknown mode \"" + mode + "\"");
        return juce::Result::ok();
    }

    juce::Result parseLsb (const juce::DynamicObject& o, Mapping& m)
    {
        if (m.mode != MapMode::Absolute14)
            return juce::Result::ok();
        if (m.kind != MidiEvent::Kind::CC)
            return juce::Result::fail ("absolute14 needs type cc");
        m.lsbNumber = o.hasProperty ("lsb") ? static_cast<int> (o.getProperty ("lsb")) : m.number + 32;
        if (m.lsbNumber < 0 || m.lsbNumber > 127 || m.lsbNumber == m.number)
            return juce::Result::fail ("lsb must be 0-127 and differ from number");
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
        return parseLsb (o, m);
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

    juce::Result parseOutputs (const juce::var& list, MidiMap& result)
    {
        for (int i = 0; i < list.size(); ++i)
        {
            OutputRule rule;
            if (const auto r = OutputRule::parse (list[i], rule); r.failed())
                return juce::Result::fail ("output " + juce::String (i) + ": " + r.getErrorMessage());
            result.outputs.push_back (std::move (rule));
        }
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
    result.version = root->hasProperty ("version") ? static_cast<int> (root->getProperty ("version")) : 1;
    if (result.version < 1 || result.version > kVersion)
        return juce::Result::fail ("Map version " + juce::String (result.version) + " is not supported");
    result.name = root->getProperty ("name").toString();
    for (int i = 0; i < list.size(); ++i)
    {
        Mapping m;
        if (const auto r = parseMapping (list[i], m); r.failed())
            return juce::Result::fail ("mapping " + juce::String (i) + ": " + r.getErrorMessage());
        result.mappings.push_back (std::move (m));
    }
    if (const auto r = parseOutputs (root->getProperty ("outputs"), result); r.failed())
        return r;
    out = std::move (result);
    return juce::Result::ok();
}

bool Mapping::operator== (const Mapping& o) const noexcept
{
    return kind == o.kind && channel == o.channel && number == o.number && target == o.target && mode == o.mode
        && min == o.min && max == o.max && step == o.step && initial == o.initial && invert == o.invert
        && lsbNumber == o.lsbNumber;
}

juce::Result Mapping::validate() const
{
    if (target.isEmpty())
        return juce::Result::fail ("missing target");
    if (channel < 0 || channel > 16 || number < 0 || number > 127)
        return juce::Result::fail ("channel must be 0-16 and number 0-127");
    if (min == max)
        return juce::Result::fail ("min and max must differ");
    if (mode == MapMode::Absolute14 && (kind != MidiEvent::Kind::CC || lsbNumber < 0 || lsbNumber > 127 || lsbNumber == number))
        return juce::Result::fail ("absolute14 needs a cc with a distinct lsb");
    return juce::Result::ok();
}

juce::String toString (MapMode mode)
{
    switch (mode)
    {
        case MapMode::Absolute:               return "absolute";
        case MapMode::Relative:               return "relative";
        case MapMode::RelativeTwosComplement: return "relative2c";
        case MapMode::Button:                 return "button";
        case MapMode::Toggle:                 return "toggle";
        case MapMode::Absolute14:             return "absolute14";
    }
    return "absolute";
}

} // namespace openmixer
