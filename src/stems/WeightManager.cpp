#include "WeightManager.h"

#include "analysis/ClipSource.h"

#include <juce_cryptography/juce_cryptography.h>

namespace openmixer::stems
{
namespace
{
constexpr int kTimeoutMs = 15000;
constexpr int kChunk = 1 << 16;

juce::String licenseDigest (const ModelManifest& m)
{
    return juce::SHA256 (m.licenseText.toUTF8()).toHexString();
}

juce::Result copyStream (juce::InputStream& in, juce::OutputStream& out, const std::function<bool (float)>& onProgress)
{
    const auto total = in.getTotalLength();
    juce::HeapBlock<char> buffer (kChunk);
    juce::int64 written = 0;
    while (! in.isExhausted())
    {
        const auto n = in.read (buffer.getData(), kChunk);
        if (n < 0 || (n > 0 && ! out.write (buffer.getData(), static_cast<size_t> (n))))
            return juce::Result::fail ("write failed");
        if (n == 0)
            break;
        written += n;
        if (onProgress && ! onProgress (total > 0 ? static_cast<float> (written) / static_cast<float> (total) : 0.0f))
            return juce::Result::fail ("cancelled");
    }
    return juce::Result::ok();
}
} // namespace

juce::Result UrlDownloader::download (const juce::String& url, const juce::File& destination,
                                      const std::function<bool (float)>& onProgress)
{
    const juce::URL source (url);
    auto in = source.isLocalFile()
                  ? std::unique_ptr<juce::InputStream> (source.getLocalFile().createInputStream())
                  : source.createInputStream (juce::URL::InputStreamOptions (juce::URL::ParameterHandling::inAddress)
                                                  .withConnectionTimeoutMs (kTimeoutMs));
    if (in == nullptr)
        return juce::Result::fail ("cannot open " + url);

    destination.deleteFile();
    juce::FileOutputStream out (destination);
    if (! out.openedOk())
        return juce::Result::fail ("cannot write " + destination.getFullPathName());
    auto result = copyStream (*in, out, onProgress);
    out.flush();
    return result;
}

WeightManager::WeightManager (juce::File modelFolder, std::unique_ptr<Downloader> downloaderToUse)
    : folder (std::move (modelFolder)), downloader (std::move (downloaderToUse))
{
}

bool WeightManager::isInstalled (const ModelManifest& m) const
{
    return getModelFile (m).existsAsFile() && markerFile (m).loadFileAsString().trim() == m.sha256;
}

bool WeightManager::hasAcceptedLicense (const ModelManifest& m) const
{
    return acceptanceFile (m).loadFileAsString().trim() == licenseDigest (m);
}

juce::Result WeightManager::ensureAvailable (const ModelManifest& m, const LicensePrompt& prompt,
                                             analysis::AnalysisContext& context)
{
    if (isInstalled (m))
        return juce::Result::ok();
    if (! folder.createDirectory())
        return juce::Result::fail ("cannot create " + folder.getFullPathName());
    if (auto accepted = acceptLicense (m, prompt); accepted.failed())
        return accepted;
    return fetchAndVerify (m, context);
}

juce::Result WeightManager::acceptLicense (const ModelManifest& m, const LicensePrompt& prompt)
{
    if (hasAcceptedLicense (m))
        return juce::Result::ok();
    if (! prompt || ! prompt (m))
        return juce::Result::fail ("licence not accepted for " + m.id);
    acceptanceFile (m).replaceWithText (licenseDigest (m));
    return juce::Result::ok();
}

juce::Result WeightManager::fetchAndVerify (const ModelManifest& m, analysis::AnalysisContext& context)
{
    if (downloader == nullptr)
        return juce::Result::fail ("no downloader");
    const auto part = folder.getChildFile (m.id + ".part");
    auto result = downloader->download (m.url, part, [&context] (float p) {
        context.setProgress (p);
        return ! context.isCancelled();
    });

    if (result.wasOk() && m.sizeBytes > 0 && part.getSize() != m.sizeBytes)
        result = juce::Result::fail ("size mismatch for " + m.id);
    if (result.wasOk() && analysis::hashFileContent (part) != m.sha256)
        result = juce::Result::fail ("checksum mismatch for " + m.id);
    if (result.wasOk() && ! part.moveFileTo (getModelFile (m)))
        result = juce::Result::fail ("cannot install " + m.id);

    part.deleteFile();
    if (result.wasOk())
        markerFile (m).replaceWithText (m.sha256);
    return result;
}

} // namespace openmixer::stems
