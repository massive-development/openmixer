#include "PlistReader.h"

namespace openmixer::library
{

PlistDict::PlistDict (const juce::XmlElement& dict)
{
    const juce::XmlElement* keyElement = nullptr;
    for (auto* child : dict.getChildIterator())
    {
        if (child->hasTagName ("key"))
            keyElement = child;
        else if (keyElement != nullptr)
        {
            const auto key = keyElement->getAllSubText();
            values[key] = child;
            ordered.emplace_back (key, child);
            keyElement = nullptr;
        }
    }
}

const juce::XmlElement* PlistDict::find (const juce::String& key) const
{
    const auto it = values.find (key);
    return it == values.end() ? nullptr : it->second;
}

juce::String PlistDict::string (const juce::String& key) const
{
    const auto* e = find (key);
    return e != nullptr ? e->getAllSubText() : juce::String();
}

juce::int64 PlistDict::integer (const juce::String& key, juce::int64 fallback) const
{
    const auto* e = find (key);
    return e != nullptr && e->hasTagName ("integer") ? e->getAllSubText().trim().getLargeIntValue() : fallback;
}

bool PlistDict::boolean (const juce::String& key) const
{
    const auto* e = find (key);
    return e != nullptr && e->hasTagName ("true");
}

juce::int64 PlistDict::dateMs (const juce::String& key) const
{
    const auto text = string (key);
    return text.isEmpty() ? 0 : juce::Time::fromISO8601 (text).toMilliseconds();
}

} // namespace openmixer::library
