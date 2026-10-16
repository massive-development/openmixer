#include "MidiTrace.h"

namespace openmixer
{

juce::Result MidiTrace::fromJson (const juce::String& json, MidiTrace& out)
{
    auto parsed = juce::JSON::parse (json);
    auto* root = parsed.getDynamicObject();
    if (root == nullptr)
        return juce::Result::fail ("Trace is not a JSON object");
    auto list = root->getProperty ("events");
    if (! list.isArray())
        return juce::Result::fail ("Trace has no \"events\" array");

    MidiTrace t;
    t.name = root->getProperty ("name").toString();
    for (int i = 0; i < list.size(); ++i)
    {
        auto* o = list[i].getDynamicObject();
        if (o == nullptr)
            return juce::Result::fail ("event " + juce::String (i) + ": not an object");

        juce::MemoryBlock bytes;
        bytes.loadFromHexString (o->getProperty ("bytes").toString());
        if (bytes.getSize() == 0)
            return juce::Result::fail ("event " + juce::String (i) + ": missing or invalid bytes");

        const auto* raw = static_cast<const juce::uint8*> (bytes.getData());
        if ((raw[0] & 0x80) == 0)
            return juce::Result::fail ("event " + juce::String (i) + ": first byte is not a status byte");
        if (raw[0] != 0xf0 && juce::MidiMessage::getMessageLengthFromFirstByte (raw[0]) != (int) bytes.getSize())
            return juce::Result::fail ("event " + juce::String (i) + ": wrong message length");

        int used = 0;
        juce::MidiMessage m (bytes.getData(), (int) bytes.getSize(), used, 0, 0.0);
        if (used != (int) bytes.getSize())
            return juce::Result::fail ("event " + juce::String (i) + ": malformed MIDI message");
        t.add ((double) o->getProperty ("time"), m);
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
    auto* root = new juce::DynamicObject();
    root->setProperty ("name", name);
    juce::Array<juce::var> list;
    for (auto& e : events)
    {
        auto* o = new juce::DynamicObject();
        o->setProperty ("time", e.time);
        o->setProperty ("bytes", juce::String::toHexString (e.message.getRawData(), e.message.getRawDataSize()).toUpperCase());
        list.add (juce::var (o));
    }
    root->setProperty ("events", list);
    return juce::JSON::toString (juce::var (root));
}

double RecordingTarget::last (const juce::String& id, double fallback) const
{
    for (auto it = changes.rbegin(); it != changes.rend(); ++it)
        if (it->id == id)
            return it->value;
    return fallback;
}

int RecordingTarget::count (const juce::String& id) const
{
    int n = 0;
    for (auto& c : changes)
        if (c.id == id)
            ++n;
    return n;
}

int replayTrace (const MidiTrace& trace, MidiMapper& mapper)
{
    int total = 0;
    for (auto& e : trace.events)
        total += mapper.handleMessage (e.message);
    return total;
}

} // namespace openmixer
