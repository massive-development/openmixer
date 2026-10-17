// OpenMixer StemsTest - model manifests, licence-gated weight download and verification, stem types
#include <JuceHeader.h>
#include "analysis/ClipSource.h"
#include "stems/StemSeparator.h"
#include "stems/WeightManager.h"

#include <gtest/gtest.h>

using namespace openmixer::stems;

namespace
{
// Real UrlDownloader (file:// source) that counts calls.
struct CountingDownloader final : Downloader
{
    explicit CountingDownloader (int& counter) : calls (counter) {}
    juce::Result download (const juce::String& url, const juce::File& destination,
                           const std::function<bool (float)>& onProgress) override
    {
        ++calls;
        return UrlDownloader().download (url, destination, onProgress);
    }
    int& calls;
};

struct CancelledContext final : openmixer::analysis::AnalysisContext
{
    bool isCancelled() const noexcept override { return true; }
    void setProgress (float) noexcept override {}
};

struct TempFolder
{
    TempFolder() { dir.createDirectory(); }
    ~TempFolder() { dir.deleteRecursively(); }
    juce::File getFile() const { return dir; }
    juce::File dir = juce::File::getSpecialLocation (juce::File::tempDirectory).getNonexistentChildFile ("om-models", "");
};

struct Fixture
{
    Fixture()
    {
        source.replaceWithText (juce::String::repeatedString ("fake weights ", 20000));
        manifest = { "demo-model.onnx", juce::URL (source).toString (false), openmixer::analysis::hashFileContent (source),
                     source.getSize(), "MIT", "Permission is hereby granted ..." };
    }

    WeightManager makeManager() { return WeightManager (folder.getFile(), std::make_unique<CountingDownloader> (downloads)); }

    juce::TemporaryFile sourceTemp { ".bin" };
    juce::File source = sourceTemp.getFile();
    TempFolder folder;
    ModelManifest manifest;
    int downloads = 0, prompts = 0;
    openmixer::analysis::NullAnalysisContext context;
    LicensePrompt accept = [this] (const ModelManifest&) { ++prompts; return true; };
};
} // namespace

TEST (ModelManifest, ParsesAndValidates)
{
    const auto json = R"({"id":"htdemucs.onnx","url":"https://example.org/m.onnx","sha256":")" + juce::String::repeatedString ("ab", 32)
                      + R"(","sizeBytes":42,"licenseName":"MIT","licenseText":"text"})";
    const auto m = ModelManifest::fromJson (json);
    ASSERT_TRUE (m.has_value());
    EXPECT_EQ (m->sizeBytes, 42);
    EXPECT_TRUE (ModelManifest::fromJson (m->toJson()).has_value());

    juce::String error;
    EXPECT_FALSE (ModelManifest::fromJson (json.replace ("htdemucs.onnx", "../evil"), &error).has_value());
    EXPECT_TRUE (error.contains ("id"));
    EXPECT_FALSE (ModelManifest::fromJson (json.replace ("abab", "zz"), &error).has_value());
    EXPECT_FALSE (ModelManifest::fromJson (json.replace ("\"text\"", "\"\""), &error).has_value());
    EXPECT_FALSE (ModelManifest::fromJson ("[]").has_value());
}

TEST (WeightManager, RequiresLicenceBeforeDownloading)
{
    Fixture f;
    auto manager = f.makeManager();
    const auto declined = manager.ensureAvailable (f.manifest, [] (const ModelManifest&) { return false; }, f.context);
    EXPECT_TRUE (declined.failed());
    EXPECT_EQ (f.downloads, 0);
    EXPECT_FALSE (manager.getModelFile (f.manifest).exists());
    EXPECT_TRUE (manager.ensureAvailable (f.manifest, {}, f.context).failed());
}

TEST (WeightManager, DownloadsVerifiesOnceAndRemembersAcceptance)
{
    Fixture f;
    auto manager = f.makeManager();
    ASSERT_TRUE (manager.ensureAvailable (f.manifest, f.accept, f.context).wasOk());
    EXPECT_TRUE (manager.isInstalled (f.manifest));
    EXPECT_TRUE (manager.hasAcceptedLicense (f.manifest));
    EXPECT_EQ (manager.getModelFile (f.manifest).loadFileAsString(), f.source.loadFileAsString());

    ASSERT_TRUE (manager.ensureAvailable (f.manifest, f.accept, f.context).wasOk());
    EXPECT_EQ (f.downloads, 1);
    EXPECT_EQ (f.prompts, 1);

    auto changedLicence = f.manifest;
    changedLicence.licenseText = "new terms";
    EXPECT_FALSE (manager.hasAcceptedLicense (changedLicence));
}

TEST (WeightManager, RejectsCorruptOrCancelledDownloads)
{
    Fixture f;
    auto manager = f.makeManager();
    auto corrupt = f.manifest;
    corrupt.sha256 = juce::String::repeatedString ("0", 64);
    const auto bad = manager.ensureAvailable (corrupt, f.accept, f.context);
    EXPECT_TRUE (bad.getErrorMessage().contains ("checksum"));
    EXPECT_FALSE (manager.getModelFile (corrupt).exists());
    EXPECT_FALSE (f.folder.getFile().getChildFile (corrupt.id + ".part").exists());

    auto wrongSize = f.manifest;
    wrongSize.sizeBytes += 1;
    EXPECT_TRUE (manager.ensureAvailable (wrongSize, f.accept, f.context).getErrorMessage().contains ("size"));

    CancelledContext cancelled;
    EXPECT_TRUE (manager.ensureAvailable (f.manifest, f.accept, cancelled).failed());
    EXPECT_FALSE (manager.isInstalled (f.manifest));

    auto missing = f.manifest;
    missing.url = juce::URL (juce::File ("/nonexistent/weights.onnx")).toString (false);
    EXPECT_TRUE (manager.ensureAvailable (missing, f.accept, f.context).failed());
}

TEST (StemSet, CompletenessAndNames)
{
    StemSet set { 44100.0, {} };
    for (auto kind : { StemKind::Drums, StemKind::Bass, StemKind::Vocals })
        set.stems[kind].assign (10, 0.0f);
    EXPECT_FALSE (set.isComplete (10));
    set.stems[StemKind::Other].assign (10, 0.0f);
    EXPECT_TRUE (set.isComplete (10));
    EXPECT_FALSE (set.isComplete (11));
    EXPECT_EQ (toString (StemKind::Vocals), "vocals");
}
