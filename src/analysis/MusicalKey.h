// Musical key value type with names and Camelot wheel notation (number 1-12 + A minor / B major).
// Pitch classes: 0 = C ... 11 = B.
#pragma once

#include <juce_core/juce_core.h>

#include <optional>

namespace openmixer::analysis
{

struct MusicalKey
{
    int tonic = 0;
    bool minor = false;

    bool operator== (const MusicalKey& other) const noexcept { return tonic == other.tonic && minor == other.minor; }
    bool operator!= (const MusicalKey& other) const noexcept { return ! (*this == other); }

    juce::String getName() const;      // "C", "F#m"
    int getCamelotNumber() const noexcept;
    juce::String getCamelot() const;   // "8B"
    MusicalKey getRelative() const noexcept;

    static std::optional<MusicalKey> fromCamelot (const juce::String& code);
    static std::optional<MusicalKey> fromName (const juce::String& name);
};

// Same slot, +-1 on the wheel, or the relative key: the usual harmonic-mixing moves.
bool isHarmonicallyCompatible (const MusicalKey& a, const MusicalKey& b) noexcept;

} // namespace openmixer::analysis
