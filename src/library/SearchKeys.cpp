#include "SearchKeys.h"

#include "analysis/MusicalKey.h"

namespace openmixer::library
{
namespace
{
std::optional<analysis::MusicalKey> resolve (const juce::String& text)
{
    const auto t = text.trim();
    if (auto k = analysis::MusicalKey::fromCamelot (t.toUpperCase()))
        return k;
    return analysis::MusicalKey::fromName (t);
}
} // namespace

juce::StringArray keyCamelotCodes (const juce::String& text, bool compatible)
{
    juce::StringArray codes;
    const auto base = resolve (text);
    if (! base)
        return codes;

    codes.add (base->getCamelot());
    if (! compatible)
        return codes;

    for (int tonic = 0; tonic < 12; ++tonic)
        for (bool minor : { false, true })
        {
            const analysis::MusicalKey candidate { tonic, minor };
            if (analysis::isHarmonicallyCompatible (*base, candidate))
                codes.addIfNotAlreadyThere (candidate.getCamelot());
        }
    return codes;
}

} // namespace openmixer::library
