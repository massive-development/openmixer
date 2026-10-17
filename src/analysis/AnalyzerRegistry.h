// Id -> analyser map shared by the service and plugins. Thread-safe; running jobs keep their
// analyser alive (shared_ptr) even if it is removed meanwhile.
#pragma once

#include "Analyzer.h"

#include <map>
#include <memory>
#include <mutex>

namespace openmixer::analysis
{

class AnalyzerRegistry
{
public:
    AnalyzerRegistry() = default;

    juce::Result add (std::shared_ptr<const Analyzer> analyzer);
    bool remove (const juce::String& id);
    std::shared_ptr<const Analyzer> find (const juce::String& id) const;
    juce::StringArray getIds() const;

private:
    mutable std::mutex mutex;
    std::map<juce::String, std::shared_ptr<const Analyzer>> analyzers;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR (AnalyzerRegistry)
};

} // namespace openmixer::analysis
