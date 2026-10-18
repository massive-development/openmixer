#include "MidiOutputRule.h"

#include <algorithm>
#include <cmath>

namespace openmixer
{

namespace
{
    juce::Result parseFields (const juce::DynamicObject& o, OutputRule& r)
    {
        r.source = o.getProperty ("source").toString();
        if (r.source.isEmpty())
            return juce::Result::fail ("missing source");

        const auto type = o.getProperty ("type").toString().toLowerCase();
        if (type == "note")    r.kind = MidiEvent::Kind::Note;
        else if (type == "cc") r.kind = MidiEvent::Kind::CC;
        else return juce::Result::fail ("output type must be note or cc");

        r.channel = o.hasProperty ("channel") ? static_cast<int> (o.getProperty ("channel")) : 1;
        r.number = static_cast<int> (o.getProperty ("number"));
        r.onValue = o.hasProperty ("on") ? static_cast<int> (o.getProperty ("on")) : 127;
        r.offValue = o.hasProperty ("off") ? static_cast<int> (o.getProperty ("off")) : 0;
        if (r.channel < 1 || r.channel > 16 || r.number < 0 || r.number > 127
            || r.onValue < 0 || r.onValue > 127 || r.offValue < 0 || r.offValue > 127)
            return juce::Result::fail ("channel must be 1-16, number/on/off 0-127");
        return juce::Result::ok();
    }

    juce::Result parseBehaviour (const juce::DynamicObject& o, OutputRule& r)
    {
        const auto mode = o.getProperty ("mode").toString().toLowerCase();
        if (mode == "level")                        r.mode = OutputRule::Mode::Level;
        else if (mode.isNotEmpty() && mode != "bool") return juce::Result::fail ("unknown output mode \"" + mode + "\"");

        const auto blink = o.getProperty ("blink").toString().toLowerCase();
        if (blink == "on")        r.blink = OutputRule::Blink::WhenOn;
        else if (blink == "off")  r.blink = OutputRule::Blink::WhenOff;
        else if (blink.isNotEmpty()) return juce::Result::fail ("blink must be \"on\" or \"off\"");

        if (o.hasProperty ("threshold")) r.threshold = static_cast<double> (o.getProperty ("threshold"));
        if (o.hasProperty ("min")) r.min = static_cast<double> (o.getProperty ("min"));
        if (o.hasProperty ("max")) r.max = static_cast<double> (o.getProperty ("max"));
        if (o.hasProperty ("blinkMs")) r.blinkMs = static_cast<int> (o.getProperty ("blinkMs"));
        if (r.min == r.max || r.blinkMs < 1)
            return juce::Result::fail ("min and max must differ, blinkMs must be positive");
        return juce::Result::ok();
    }
}

juce::Result OutputRule::parse (const juce::var& v, OutputRule& out)
{
    const auto* o = v.getDynamicObject();
    if (o == nullptr)
        return juce::Result::fail ("not an object");

    OutputRule r;
    for (auto step : { &parseFields, &parseBehaviour })
        if (const auto res = step (*o, r); res.failed())
            return res;
    out = std::move (r);
    return juce::Result::ok();
}

juce::var OutputRule::toVar() const
{
    auto* o = new juce::DynamicObject();
    o->setProperty ("source", source);
    o->setProperty ("type", kind == MidiEvent::Kind::CC ? "cc" : "note");
    o->setProperty ("channel", channel);
    o->setProperty ("number", number);
    o->setProperty ("on", onValue);
    o->setProperty ("off", offValue);
    if (mode == Mode::Level)
    {
        o->setProperty ("mode", "level");
        o->setProperty ("min", min);
        o->setProperty ("max", max);
        return juce::var (o);
    }
    o->setProperty ("threshold", threshold);
    if (blink != Blink::None)
    {
        o->setProperty ("blink", blink == Blink::WhenOn ? "on" : "off");
        o->setProperty ("blinkMs", blinkMs);
    }
    return juce::var (o);
}

int OutputRule::evaluate (double state, double nowMs) const noexcept
{
    if (mode == Mode::Level)
    {
        const double norm = std::clamp ((state - min) / (max - min), 0.0, 1.0);
        return static_cast<int> (std::lround (offValue + norm * (onValue - offValue)));
    }
    const bool on = state >= threshold;
    const bool blinking = blink == Blink::WhenOn ? on : (blink == Blink::WhenOff && ! on);
    if (! blinking)
        return on ? onValue : offValue;
    const auto phase = static_cast<long long> (std::floor (nowMs / blinkMs));
    return phase % 2 == 0 ? onValue : offValue;   // lit on even half periods, so the blink is the same on every rule
}

juce::MidiMessage OutputRule::toMessage (int value) const
{
    const auto v = static_cast<juce::uint8> (std::clamp (value, 0, 127));
    if (kind == MidiEvent::Kind::CC)
        return juce::MidiMessage::controllerEvent (channel, number, v);
    return v > 0 ? juce::MidiMessage::noteOn (channel, number, v) : juce::MidiMessage::noteOff (channel, number);
}

} // namespace openmixer
