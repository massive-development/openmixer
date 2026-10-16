#include "EngineTarget.h"

namespace openmixer
{

void EngineTarget::setParameter (const juce::String& id, double value)
{
    const auto parts = juce::StringArray::fromTokens (id, ".", "");
    const float v = (float) value;
    bool ok = false;

    if (parts.size() >= 2 && parts[0] == "mixer")
    {
        if (parts[1].startsWith ("ch") && parts.size() >= 3 && parts[1].substring (2).containsOnly ("0123456789")
            && parts[1].length() > 2)
        {
            const int ch = parts[1].substring (2).getIntValue();
            if (ch < mixer.getNumChannels())
            {
                const auto& p = parts[2];
                ok = true;
                if (p == "trim")        mixer.setTrim (ch, v);
                else if (p == "fader")  mixer.setFader (ch, v);
                else if (p == "filter") mixer.setFilter (ch, v);
                else if (p == "pfl")    mixer.setPfl (ch, v >= 0.5f);
                else if (p == "xfader") mixer.setCrossfaderAssign (ch, (CrossfaderAssign) juce::jlimit (0, 2, (int) std::lround (value)));
                else if (p == "eq" && parts.size() == 4 && parts[3] == "low")  mixer.setEq (ch, 0, v);
                else if (p == "eq" && parts.size() == 4 && parts[3] == "mid")  mixer.setEq (ch, 1, v);
                else if (p == "eq" && parts.size() == 4 && parts[3] == "high") mixer.setEq (ch, 2, v);
                else ok = false;
            }
        }
        else if (parts.size() == 2 && parts[1] == "crossfader") { mixer.setCrossfader (v); ok = true; }
        else if (parts.size() == 2 && parts[1] == "master")     { mixer.setMasterGain (v); ok = true; }
        else if (parts.size() == 3 && parts[1] == "cue" && parts[2] == "level") { mixer.setCueLevel (v); ok = true; }
        else if (parts.size() == 3 && parts[1] == "cue" && parts[2] == "mix")   { mixer.setCueMix (v); ok = true; }
    }
    else if (parts.size() == 3 && parts[0] == "deck" && parts[1].containsOnly ("0123456789") && parts[1].isNotEmpty())
    {
        const int d = parts[1].getIntValue();
        if (d < (int) decks.size() && decks[(size_t) d] != nullptr)
        {
            auto& deck = *decks[(size_t) d];
            ok = true;
            if (parts[2] == "play")      deck.setPlaying (v >= 0.5f);
            else if (parts[2] == "rate") deck.setRate (value);
            else if (parts[2] == "seek") deck.seek ((juce::int64) (juce::jlimit (0.0, 1.0, value) * (double) deck.getLengthFrames()));
            else ok = false;
        }
    }

    if (! ok)
        unknown.fetch_add (1);
}

} // namespace openmixer
