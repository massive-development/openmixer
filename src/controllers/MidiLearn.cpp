#include "MidiLearn.h"

#include <algorithm>

namespace openmixer
{

namespace
{
    bool endsWithAny (const juce::String& id, std::initializer_list<const char*> tails)
    {
        return std::any_of (tails.begin(), tails.end(), [&] (const char* t) { return id.endsWith (t); });
    }

    MapMode modeFor (LearnType t, MidiEvent::Kind kind)
    {
        switch (t)
        {
            case LearnType::Relative:               return MapMode::Relative;
            case LearnType::RelativeTwosComplement: return MapMode::RelativeTwosComplement;
            case LearnType::Button:                 return MapMode::Button;
            case LearnType::Toggle:                 return MapMode::Toggle;
            case LearnType::Absolute14:             return MapMode::Absolute14;
            case LearnType::Absolute:               return MapMode::Absolute;
            case LearnType::Auto:                   break;
        }
        return kind == MidiEvent::Kind::Note ? MapMode::Button : MapMode::Absolute;
    }

    // Absolute14 binds the MSB (0-31) with its LSB 32 higher; a control that sent the LSB first is accepted too.
    bool resolve14 (Mapping& m)
    {
        if (m.kind != MidiEvent::Kind::CC)
            return false;
        if (m.number >= 32 && m.number < 64)
            m.number -= 32;
        if (m.number >= 32)
            return false;
        m.lsbNumber = m.number + 32;
        return true;
    }
}

LearnRequest LearnRequest::forTarget (const juce::String& id)
{
    LearnRequest r;
    r.target = id;
    if (endsWithAny (id, { ".eq.low", ".eq.mid", ".eq.high" }))   { r.min = -48.0; r.max = 6.0; r.initial = 0.0; }
    else if (id.endsWith (".filter"))                              { r.min = -1.0; r.max = 1.0; r.initial = 0.0; }
    else if (id.endsWith (".rate"))                                { r.min = 0.92; r.max = 1.08; r.initial = 1.0; }
    else if (id == "mixer.master" || id == "mixer.cue.level")      { r.max = 2.0; }
    else if (id.endsWith (".trim"))                                { r.max = 4.0; r.initial = 1.0; }
    if (endsWithAny (id, { ".play", ".keylock", ".sync", ".sync.master", ".slip", ".quantize" }))
        r.type = LearnType::Toggle;
    else if (endsWithAny (id, { ".pfl", ".clear", ".loop.exit", ".loop.reloop" }) || id.contains (".hotcue") || id.contains (".loop.quantized."))
        r.type = LearnType::Button;
    return r;
}

void MidiLearn::arm (const LearnRequest& r)
{
    const std::lock_guard<std::mutex> guard (lock);
    request = r;
    armed.store (! r.target.isEmpty(), std::memory_order_release);
}

void MidiLearn::cancel()
{
    const std::lock_guard<std::mutex> guard (lock);
    armed.store (false, std::memory_order_release);
}

LearnRequest MidiLearn::getRequest() const
{
    const std::lock_guard<std::mutex> guard (lock);
    return request;
}

std::optional<Mapping> MidiLearn::capture (const MidiEvent& e)
{
    if (! isArmed() || (e.kind == MidiEvent::Kind::Note && ! e.noteOn))
        return std::nullopt;

    const std::lock_guard<std::mutex> guard (lock);
    if (! armed.load (std::memory_order_relaxed))
        return std::nullopt;

    Mapping m;
    m.kind = e.kind;
    m.channel = request.anyChannel ? 0 : e.channel;
    m.number = e.number;
    m.target = request.target;
    m.mode = modeFor (request.type, e.kind);
    m.min = request.min;
    m.max = request.max;
    m.initial = request.initial;
    m.invert = request.invert;
    m.step = request.step >= 0.0 ? request.step : (request.max - request.min) / 127.0;
    if (m.mode == MapMode::Absolute14 && ! resolve14 (m))
        return std::nullopt;   // stays armed: the user can wiggle another control
    armed.store (false, std::memory_order_release);
    return m;
}

} // namespace openmixer
