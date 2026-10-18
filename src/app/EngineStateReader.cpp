#include "EngineStateReader.h"

#include <algorithm>

namespace openmixer
{

namespace
{
    bool parseIndex (const juce::String& s, const juce::String& prefix, int& out)
    {
        const auto digits = s.substring (prefix.length());
        if (! s.startsWith (prefix) || digits.isEmpty() || ! digits.containsOnly ("0123456789"))
            return false;
        out = digits.getIntValue();
        return true;
    }

    double flag (bool b) noexcept { return b ? 1.0 : 0.0; }
}

bool EngineStateReader::readState (const juce::String& id, double& out) const
{
    const auto parts = juce::StringArray::fromTokens (id, ".", "");
    if (parts.size() < 2)
        return false;
    if (parts[0] == "deck")
        return readDeck (parts, out);
    return parts[0] == "mixer" && readMixer (parts, out);
}

bool EngineStateReader::readDeck (const juce::StringArray& parts, double& out) const
{
    int index = 0;
    if (parts.size() < 3 || ! parseIndex ("n" + parts[1], "n", index) || index >= engine.getNumDecks())
        return false;
    const auto& deck = engine.getDeck (index);
    const auto& control = engine.getDeckControl (index);
    const auto what = parts.joinIntoString (".", 2);
    int cue = 0;

    if (what == "play")             out = flag (deck.isPlaying());
    else if (what == "loaded")      out = flag (deck.isLoaded());
    else if (what == "keylock")     out = flag (deck.isKeylock());
    else if (what == "loop")        out = flag (deck.isLooping());
    else if (what == "cue")         out = flag (deck.isLoaded() && ! deck.isPlaying());
    else if (what == "sync")        out = flag (engine.getSync().isEnabled (index));
    else if (what == "sync.master") out = flag (engine.getSync().getLeader() == index);
    else if (what == "slip")        out = flag (control.isSlipping());
    else if (what == "quantize")    out = flag (control.isQuantize());
    else if (what == "rate")        out = deck.getRate();
    else if (what == "position")    out = deck.getLengthFrames() > 0 ? static_cast<double> (deck.getPositionFrames()) / static_cast<double> (deck.getLengthFrames()) : 0.0;
    else if (what.endsWith (".set") && parseIndex (what.upToLastOccurrenceOf (".", false, false), "hotcue", cue))
        out = flag (deck.getHotcue (cue).has_value());
    else
        return false;
    return true;
}

bool EngineStateReader::readChannel (int ch, const juce::String& what, double& out) const
{
    const auto& mixer = engine.getMixer();
    if (what == "pfl")         out = flag (mixer.getPfl (ch));
    else if (what == "fader")  out = static_cast<double> (mixer.getFader (ch));
    else if (what == "trim")   out = static_cast<double> (mixer.getTrim (ch));
    else if (what == "filter") out = static_cast<double> (mixer.getFilter (ch));
    else if (what == "level")  out = static_cast<double> (mixer.getChannelPeak (ch));
    else return false;
    return true;
}

bool EngineStateReader::readMixer (const juce::StringArray& parts, double& out) const
{
    const auto& mixer = engine.getMixer();
    int ch = 0;
    if (parseIndex (parts[1], "ch", ch))
        return ch < mixer.getNumChannels() && parts.size() == 3 && readChannel (ch, parts[2], out);

    const auto what = parts.joinIntoString (".", 1);
    if (what == "crossfader")         out = static_cast<double> (mixer.getCrossfader());
    else if (what == "master")        out = static_cast<double> (mixer.getMasterGain());
    else if (what == "master.level")  out = static_cast<double> (std::max (mixer.getMasterPeakLeft(), mixer.getMasterPeakRight()));
    else return false;
    return true;
}

} // namespace openmixer
