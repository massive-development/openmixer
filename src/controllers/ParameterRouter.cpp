#include "ParameterRouter.h"

namespace openmixer
{

void ParameterRouter::registerTarget (const juce::String& prefix, ParameterTarget* target)
{
    targets[prefix] = target;
}

void ParameterRouter::unregisterTarget (const juce::String& prefix)
{
    targets.erase (prefix);
}

bool ParameterRouter::setParameter (const juce::String& id, double value)
{
    // Longest registered prefix wins ("deck.0" beats "deck").
    const juce::String* bestPrefix = nullptr;
    ParameterTarget* best = nullptr;
    for (const auto& [prefix, target] : targets)
    {
        const bool matches = id.startsWith (prefix + ".") && (bestPrefix == nullptr || prefix.length() > bestPrefix->length());
        if (matches && target != nullptr)
        {
            bestPrefix = &prefix;
            best = target;
        }
    }

    const bool handled = best != nullptr && best->setParameter (id.substring (bestPrefix->length() + 1), value);
    if (! handled)
        unknown.fetch_add (1, std::memory_order_relaxed);
    return handled;
}

} // namespace openmixer
