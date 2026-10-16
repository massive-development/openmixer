#include "MidiTrace.h"

namespace openmixer
{

namespace
{
    // juce::DynamicObject is intrusively ref-counted; Ptr takes ownership immediately.
    juce::DynamicObject::Ptr makeObject() { return juce::DynamicObject::Ptr (new juce::DynamicObject()); }

    juce::Result parseBytes (const juce::String& hex, juce::MemoryBlock& bytes)
    {
        bytes.loadFromHexString (hex);
        if (bytes.getSize() == 0)
            return juce::Result::fail ("missing or invalid bytes");

        const auto* raw = static_cast<const juce::uint8*> (bytes.getData());
        if ((raw[0] & 0x80) == 0)
            return juce::Result::fail ("first byte is not a status byte");
        // Sysex length is variable, so only fixed-length messages are length-checked.
        if (raw[0] != 0xf0 && juce::MidiMessage::getMessageLengthFromFirstByte (raw[0]) != static_cast<int> (bytes.getSize()))
            return juce::Result::fail ("wrong message length");
        return juce::Result::ok();
    }

    juce::Result parseEvent (const juce::var& v, TimedMidi& out)
    {
        const auto* o = v.getDynamicObject();
        if (o == nullptr)
            return juce::Result::fail ("not an object");

        juce::MemoryBlock bytes;
        if (const auto r = parseBytes (o->getProperty ("bytes").toString(), bytes); r.failed())
            return r;

        int used = 0;
        const auto size = static_cast<int> (bytes.getSize());
        out.message = juce::MidiMessage (bytes.getData(), size, used, 0, 0.0);
        out.time = static_cast<double> (o->getProperty ("time"));
        return used == size ? juce::Result::ok() : juce::Result::fail ("malformed MIDI message");
    }
}

juce::Result MidiTrace::fromJson (const juce::String& json, MidiTrace& out)
{
    const auto parsed = juce::JSON::parse (json);
    const auto* root = parsed.getDynamicObject();
    if (root == nullptr)
        return juce::Result::fail ("Trace is not a JSON object");
    const auto list = root->getProperty ("events");
    if (! list.isArray())
        return juce::Result::fail ("Trace has no \"events\" array");

    MidiTrace t;
    t.name = root->getProperty ("name").toString();
    for (int i = 0; i < list.size(); ++i)
    {
        TimedMidi e;
        if (const auto r = parseEvent (list[i], e); r.failed())
            return juce::Result::fail ("event " + juce::String (i) + ": " + r.getErrorMessage());
        t.events.push_back (e);
    }
    out = std::move (t);
    return juce::Result::ok();
}

juce::Result MidiTrace::fromFile (const juce::File& file, MidiTrace& out)
{
    if (! file.existsAsFile())
        return juce::Result::fail ("Trace file not found: " + file.getFullPathName());
    return fromJson (file.loadFileAsString(), out);
}

juce::String MidiTrace::toJson() const
{
    juce::Array<juce::var> list;
    for (const auto& e : events)
    {
        auto o = makeObject();
        o->setProperty ("time", e.time);
        o->setProperty ("bytes", juce::String::toHexString (e.message.getRawData(), e.message.getRawDataSize()).toUpperCase());
        list.add (juce::var (o.get()));
    }
    auto root = makeObject();
    root->setProperty ("name", name);
    root->setProperty ("events", list);
    return juce::JSON::toString (juce::var (root.get()));
}

int replayTrace (const MidiTrace& trace, MidiMapper& mapper)
{
    int total = 0;
    for (const auto& e : trace.events)
        total += mapper.handleMessage (e.message);
    return total;
}

} // namespace openmixer
