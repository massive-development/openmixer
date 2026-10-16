// String-id parameter routing for Mixer (shared by MIDI, UI and tests).
#include "Mixer.h"

#include <cmath>

namespace openmixer
{

bool Mixer::setChannelParameter (const juce::StringArray& parts, double value)
{
    const auto digits = parts[0].substring (2);
    if (digits.isEmpty() || ! digits.containsOnly ("0123456789"))
        return false;
    const int ch = digits.getIntValue();
    if (ch >= numChannels || parts.size() < 2)
        return false;

    const auto v = static_cast<float> (value);
    const auto& p = parts[1];
    if (parts.size() == 2)
    {
        if (p == "trim")        setTrim (ch, v);
        else if (p == "fader")  setFader (ch, v);
        else if (p == "filter") setFilter (ch, v);
        else if (p == "pfl")    setPfl (ch, v >= 0.5F);
        else if (p == "xfader") setCrossfaderAssign (ch, static_cast<CrossfaderAssign> (juce::jlimit (0, 2, static_cast<int> (std::lround (value)))));
        else return false;
        return true;
    }
    if (parts.size() == 3 && p == "eq")
    {
        const int band = parts[2] == "low" ? 0 : (parts[2] == "mid" ? 1 : (parts[2] == "high" ? 2 : -1));
        if (band < 0)
            return false;
        setEq (ch, band, v);
        return true;
    }
    return false;
}

bool Mixer::setParameter (const juce::String& id, double value)
{
    const auto parts = juce::StringArray::fromTokens (id, ".", "");
    const auto v = static_cast<float> (value);
    if (parts.size() >= 2 && parts[0].startsWith ("ch"))
        return setChannelParameter (parts, value);
    if (parts.size() == 1 && parts[0] == "crossfader") { setCrossfader (v); return true; }
    if (parts.size() == 1 && parts[0] == "master")     { setMasterGain (v); return true; }
    if (parts.size() == 2 && parts[0] == "cue" && parts[1] == "level") { setCueLevel (v); return true; }
    if (parts.size() == 2 && parts[0] == "cue" && parts[1] == "mix")   { setCueMix (v); return true; }
    return false;
}

} // namespace openmixer
