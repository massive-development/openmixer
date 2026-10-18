// Fetches model weights on first use: the user must accept the model's licence (prompt callback),
// then the file is downloaded to "<id>.part", checked against the manifest's SHA-256 and moved into
// the model folder. A "<id>.sha256" marker avoids re-hashing large files on every start.
// Blocking; call from a background thread, one call per model at a time.
#pragma once

#include "ModelManifest.h"
#include "analysis/Analyzer.h"

#include <functional>
#include <memory>

namespace openmixer::stems
{

class Downloader
{
public:
    virtual ~Downloader() = default;

    // Writes url to destination; onProgress(0..1) returns false to abort.
    virtual juce::Result download (const juce::String& url, const juce::File& destination,
                                   const std::function<bool (float)>& onProgress) = 0;
};

// juce::URL backed (http, https and file URLs).
class UrlDownloader final : public Downloader
{
public:
    juce::Result download (const juce::String& url, const juce::File& destination,
                           const std::function<bool (float)>& onProgress) override;
};

using LicensePrompt = std::function<bool (const ModelManifest&)>;   // true = accepted

class WeightManager
{
public:
    WeightManager (juce::File modelFolder, std::unique_ptr<Downloader> downloaderToUse);

    juce::File getModelFile (const ModelManifest& m) const { return folder.getChildFile (m.id); }
    bool isInstalled (const ModelManifest& m) const;
    bool hasAcceptedLicense (const ModelManifest& m) const;

    // Ok when the verified file is present (downloading it if needed).
    juce::Result ensureAvailable (const ModelManifest& m, const LicensePrompt& prompt,
                                  analysis::AnalysisContext& context);

private:
    juce::File markerFile (const ModelManifest& m) const { return folder.getChildFile (m.id + ".sha256"); }
    juce::File acceptanceFile (const ModelManifest& m) const { return folder.getChildFile (m.id + ".license"); }
    juce::Result acceptLicense (const ModelManifest& m, const LicensePrompt& prompt);
    juce::Result fetchAndVerify (const ModelManifest& m, analysis::AnalysisContext& context);

    juce::File folder;
    std::unique_ptr<Downloader> downloader;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR (WeightManager)
};

} // namespace openmixer::stems
