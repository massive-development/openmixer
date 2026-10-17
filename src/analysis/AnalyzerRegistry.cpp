#include "AnalyzerRegistry.h"

namespace openmixer::analysis
{

juce::Result AnalyzerRegistry::add (std::shared_ptr<const Analyzer> analyzer)
{
    if (analyzer == nullptr)
        return juce::Result::fail ("null analyzer");
    if (analyzer->getInterfaceVersion() != Analyzer::kInterfaceVersion)
        return juce::Result::fail ("analyzer interface version mismatch: " + analyzer->getId());

    auto id = analyzer->getId();
    if (id.isEmpty())
        return juce::Result::fail ("analyzer id is empty");

    const std::lock_guard<std::mutex> lock (mutex);
    if (! analyzers.emplace (id, std::move (analyzer)).second)
        return juce::Result::fail ("analyzer already registered: " + id);
    return juce::Result::ok();
}

bool AnalyzerRegistry::remove (const juce::String& id)
{
    const std::lock_guard<std::mutex> lock (mutex);
    return analyzers.erase (id) > 0;
}

std::shared_ptr<const Analyzer> AnalyzerRegistry::find (const juce::String& id) const
{
    const std::lock_guard<std::mutex> lock (mutex);
    const auto it = analyzers.find (id);
    return it != analyzers.end() ? it->second : nullptr;
}

juce::StringArray AnalyzerRegistry::getIds() const
{
    const std::lock_guard<std::mutex> lock (mutex);
    juce::StringArray ids;
    for (const auto& entry : analyzers)
        ids.add (entry.first);
    return ids;
}

} // namespace openmixer::analysis
