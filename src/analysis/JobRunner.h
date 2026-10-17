// Runs one job to completion on the calling thread: hash, cache lookup, decode on a miss, analyse,
// store. Progress: 0-5% hash, 5-20% decode, 20-100% split across the analysers that must run.
#pragma once

#include "AnalysisJob.h"
#include "AnalyzerRegistry.h"
#include "ClipSource.h"
#include "ResultCache.h"

namespace openmixer::analysis
{

void runAnalysisJob (AnalysisJob& job, ClipSource& source, const juce::StringArray& analyzerIds,
                     const AnalyzerRegistry& registry, ResultCache* cache);

} // namespace openmixer::analysis
