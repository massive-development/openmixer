#include "MusicalKey.h"

#include <array>

namespace openmixer::analysis
{
namespace
{
constexpr std::array<const char*, 12> kSharpNames { "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B" };
constexpr int kCamelotOfC = 8;   // C major = 8B, A minor = 8A

int wrap12 (int v) noexcept { return ((v % 12) + 12) % 12; }

int relativeMajorTonic (const MusicalKey& k) noexcept { return k.minor ? wrap12 (k.tonic + 3) : k.tonic; }

std::optional<int> pitchClassOf (const juce::String& note)
{
    for (int pc = 0; pc < 12; ++pc)
        if (note.equalsIgnoreCase (kSharpNames[static_cast<size_t> (pc)]))
            return pc;
    if (note.length() == 2 && (note[1] == 'b' || note[1] == 'B'))
        if (const auto natural = pitchClassOf (note.substring (0, 1)))
            return wrap12 (*natural - 1);
    return std::nullopt;
}
} // namespace

juce::String MusicalKey::getName() const
{
    return juce::String (kSharpNames[static_cast<size_t> (wrap12 (tonic))]) + (minor ? "m" : "");
}

int MusicalKey::getCamelotNumber() const noexcept
{
    const auto fifthsFromC = wrap12 (relativeMajorTonic (*this) * 7);
    return wrap12 (kCamelotOfC - 1 + fifthsFromC) + 1;
}

juce::String MusicalKey::getCamelot() const
{
    return juce::String (getCamelotNumber()) + (minor ? "A" : "B");
}

MusicalKey MusicalKey::getRelative() const noexcept
{
    return minor ? MusicalKey { wrap12 (tonic + 3), false } : MusicalKey { wrap12 (tonic + 9), true };
}

std::optional<MusicalKey> MusicalKey::fromCamelot (const juce::String& code)
{
    const auto trimmed = code.trim().toUpperCase();
    const auto letter = trimmed.getLastCharacter();
    const auto number = trimmed.dropLastCharacters (1).getIntValue();
    if ((letter != 'A' && letter != 'B') || number < 1 || number > 12 || ! trimmed.dropLastCharacters (1).containsOnly ("0123456789"))
        return std::nullopt;

    const auto fifthsFromC = wrap12 (number - kCamelotOfC);
    const auto majorTonic = wrap12 (fifthsFromC * 7);   // 7 is its own inverse mod 12
    const MusicalKey major { majorTonic, false };
    return letter == 'B' ? major : major.getRelative();
}

std::optional<MusicalKey> MusicalKey::fromName (const juce::String& name)
{
    auto text = name.trim();
    const auto isMinor = text.endsWith ("m") && ! text.endsWithIgnoreCase ("maj");
    if (isMinor)
        text = text.dropLastCharacters (1);
    if (const auto pc = pitchClassOf (text))
        return MusicalKey { *pc, isMinor };
    return std::nullopt;
}

bool isHarmonicallyCompatible (const MusicalKey& a, const MusicalKey& b) noexcept
{
    const auto distance = wrap12 (a.getCamelotNumber() - b.getCamelotNumber());
    if (a.minor == b.minor)
        return distance == 0 || distance == 1 || distance == 11;
    return distance == 0;
}

} // namespace openmixer::analysis
