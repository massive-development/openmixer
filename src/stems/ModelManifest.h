// Describes a downloadable model (e.g. Demucs ONNX weights): where to fetch it, how to verify it
// and the licence the user must accept first. Weights are never committed to the repository;
// manifests are plain JSON so users and plugins can point at other models or mirrors.
#pragma once

#include <juce_core/juce_core.h>

#include <optional>

namespace openmixer::stems
{

struct ModelManifest
{
    juce::String id;            // also the file name in the model folder
    juce::String url;
    juce::String sha256;        // lowercase hex of the file
    juce::int64 sizeBytes = 0;  // 0 = unknown
    juce::String licenseName;   // e.g. "MIT"
    juce::String licenseText;   // shown to the user before downloading

    static std::optional<ModelManifest> fromJson (const juce::String& json, juce::String* error = nullptr);
    juce::String toJson() const;
};

} // namespace openmixer::stems
