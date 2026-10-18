// MidiMap::toJson: the inverse of MidiMap::parse, always at the current schema version.
#include "MidiMap.h"

namespace openmixer
{

namespace
{
    juce::String kindName (MidiEvent::Kind k)
    {
        return k == MidiEvent::Kind::CC ? "cc" : (k == MidiEvent::Kind::Note ? "note" : "pitchbend");
    }

    juce::var mappingToVar (const Mapping& m)
    {
        auto* o = new juce::DynamicObject();
        o->setProperty ("type", kindName (m.kind));
        if (m.channel != 0)
            o->setProperty ("channel", m.channel);
        if (m.kind != MidiEvent::Kind::PitchBend)
            o->setProperty ("number", m.number);
        o->setProperty ("target", m.target);
        o->setProperty ("mode", toString (m.mode));
        o->setProperty ("min", m.min);
        o->setProperty ("max", m.max);
        o->setProperty ("step", m.step);
        o->setProperty ("initial", m.initial);
        if (m.invert)
            o->setProperty ("invert", true);
        if (m.mode == MapMode::Absolute14)
            o->setProperty ("lsb", m.lsbNumber);
        return juce::var (o);
    }
}

juce::String MidiMap::toJson() const
{
    juce::Array<juce::var> maps, outs;
    for (const auto& m : mappings)
        maps.add (mappingToVar (m));
    for (const auto& r : outputs)
        outs.add (r.toVar());

    auto* root = new juce::DynamicObject();
    root->setProperty ("version", kVersion);
    root->setProperty ("name", name);
    root->setProperty ("mappings", maps);
    root->setProperty ("outputs", outs);
    return juce::JSON::toString (juce::var (root));
}

} // namespace openmixer
