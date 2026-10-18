// Typed access to an Apple property-list <dict> (alternating <key> and value elements).
#pragma once

#include <juce_core/juce_core.h>

#include <map>

namespace openmixer::library
{

class PlistDict
{
public:
    explicit PlistDict (const juce::XmlElement& dict);

    bool has (const juce::String& key) const { return values.count (key) != 0; }
    const juce::XmlElement* find (const juce::String& key) const;
    juce::String string (const juce::String& key) const;
    juce::int64 integer (const juce::String& key, juce::int64 fallback = 0) const;
    bool boolean (const juce::String& key) const;                // <true/> only
    juce::int64 dateMs (const juce::String& key) const;          // ISO-8601 <date>, 0 when absent

    // Every (key, value-element) pair in document order.
    const std::vector<std::pair<juce::String, const juce::XmlElement*>>& entries() const { return ordered; }

private:
    std::map<juce::String, const juce::XmlElement*> values;
    std::vector<std::pair<juce::String, const juce::XmlElement*>> ordered;
};

} // namespace openmixer::library
