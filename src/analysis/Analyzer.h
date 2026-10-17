// Analyser extension point: built-ins (BPM, key, ...) and plugins implement this and register it
// with AnalyzerRegistry. analyze() runs on AnalysisService worker threads, possibly concurrently,
// so implementations must be stateless or internally synchronised.
#pragma once

#include "AudioClip.h"

#include <juce_core/juce_core.h>

#include <optional>

namespace openmixer::analysis
{

class AnalysisContext
{
public:
    virtual ~AnalysisContext() = default;

    virtual bool isCancelled() const noexcept = 0;
    virtual void setProgress (float fraction) noexcept = 0;   // 0..1 within the current step
};

// Context for direct (non-service) calls: never cancelled, progress ignored.
class NullAnalysisContext final : public AnalysisContext
{
public:
    bool isCancelled() const noexcept override { return false; }
    void setProgress (float) noexcept override {}
};

// Maps a sub-step's 0..1 progress into [start, start + length] of a parent context.
class SubProgressContext final : public AnalysisContext
{
public:
    SubProgressContext (AnalysisContext& parentContext, float start, float length) noexcept
        : parent (parentContext), base (start), span (length) {}

    bool isCancelled() const noexcept override { return parent.isCancelled(); }
    void setProgress (float fraction) noexcept override
    {
        parent.setProgress (base + span * juce::jlimit (0.0f, 1.0f, fraction));
    }

private:
    AnalysisContext& parent;
    float base, span;
};

class Analyzer
{
public:
    static constexpr int kInterfaceVersion = 1;

    virtual ~Analyzer() = default;

    virtual int getInterfaceVersion() const noexcept { return kInterfaceVersion; }
    virtual juce::String getId() const = 0;
    // Bump when output changes so cached results from older versions are recomputed.
    virtual int getVersion() const = 0;

    // JSON payload for the cache; nullopt when cancelled or the clip cannot be analysed.
    virtual std::optional<juce::String> analyze (const AudioClip& clip, AnalysisContext& context) const = 0;
};

} // namespace openmixer::analysis
