#include "JobRunner.h"

#include <vector>

namespace openmixer::analysis
{
namespace
{
using AnalyzerList = std::vector<std::shared_ptr<const Analyzer>>;

constexpr float kHashEnd = 0.05f;
constexpr float kLoadEnd = 0.2f;

// Maps a step's 0..1 progress into its slice of the job's progress.
class StepContext final : public AnalysisContext
{
public:
    StepContext (AnalysisJob& j, float start, float length) noexcept : job (j), base (start), span (length) {}

    bool isCancelled() const noexcept override { return job.isCancelled(); }
    void setProgress (float fraction) noexcept override
    {
        job.setProgress (base + span * juce::jlimit (0.0f, 1.0f, fraction));
    }

private:
    AnalysisJob& job;
    float base, span;
};

std::optional<AnalyzerList> resolve (const juce::StringArray& ids, const AnalyzerRegistry& registry,
                                     juce::String& missingId)
{
    AnalyzerList list;
    for (const auto& id : ids)
    {
        auto analyzer = registry.find (id);
        if (analyzer == nullptr)
        {
            missingId = id;
            return std::nullopt;
        }
        list.push_back (std::move (analyzer));
    }
    return list;
}

AnalyzerList takeCached (AnalysisJob& job, const AnalyzerList& analyzers, ResultCache* cache, const juce::String& hash)
{
    if (cache == nullptr)
        return analyzers;

    AnalyzerList missing;
    for (const auto& a : analyzers)
    {
        if (auto hit = cache->find (hash, a->getId(), a->getVersion()))
            job.addResult (a->getId(), *hit, true);
        else
            missing.push_back (a);
    }
    return missing;
}

// False if cancelled or an analyser failed (the job is then marked accordingly).
bool runAnalyzers (AnalysisJob& job, const AudioClip& clip, const AnalyzerList& analyzers, ResultCache* cache,
                   const juce::String& hash)
{
    const auto span = (1.0f - kLoadEnd) / static_cast<float> (analyzers.size());
    for (size_t i = 0; i < analyzers.size(); ++i)
    {
        const auto& a = *analyzers[i];
        StepContext step (job, kLoadEnd + span * static_cast<float> (i), span);
        auto payload = a.analyze (clip, step);
        if (job.isCancelled())
            return false;
        if (! payload)
        {
            job.fail ("analyzer failed: " + a.getId());
            return false;
        }
        if (cache != nullptr)
            cache->store (hash, a.getId(), a.getVersion(), *payload);
        job.addResult (a.getId(), *payload, false);
    }
    return true;
}

bool analyseMissing (AnalysisJob& job, ClipSource& source, const AnalyzerList& missing, ResultCache* cache,
                     const juce::String& hash)
{
    StepContext loadStep (job, kHashEnd, kLoadEnd - kHashEnd);
    const auto clip = source.load (loadStep);
    if (job.isCancelled())
        return false;
    if (! clip)
    {
        job.fail ("cannot decode " + source.describe());
        return false;
    }
    return runAnalyzers (job, *clip, missing, cache, hash);
}

void finish (AnalysisJob& job)
{
    if (job.getStatus() == JobStatus::Failed)
        return;
    if (job.isCancelled())
    {
        job.setStatus (JobStatus::Cancelled);
        return;
    }
    job.setProgress (1.0f);
    job.setStatus (JobStatus::Done);
}

bool process (AnalysisJob& job, ClipSource& source, const AnalyzerList& analyzers, ResultCache* cache)
{
    const auto hash = source.getContentHash();
    if (hash.isEmpty())
    {
        job.fail ("cannot read " + source.describe());
        return false;
    }
    job.setContentHash (hash);
    job.setProgress (kHashEnd);

    const auto missing = takeCached (job, analyzers, cache, hash);
    return missing.empty() || analyseMissing (job, source, missing, cache, hash);
}
} // namespace

void runAnalysisJob (AnalysisJob& job, ClipSource& source, const juce::StringArray& analyzerIds,
                     const AnalyzerRegistry& registry, ResultCache* cache)
{
    if (! job.isCancelled())
    {
        job.setStatus (JobStatus::Running);
        juce::String missingId;
        if (const auto analyzers = resolve (analyzerIds, registry, missingId))
            process (job, source, *analyzers, cache);
        else
            job.fail ("unknown analyzer: " + missingId);
    }
    finish (job);
}

} // namespace openmixer::analysis
