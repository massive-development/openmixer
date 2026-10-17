#include "ModelManifest.h"

namespace openmixer::stems
{
namespace
{
bool isSafeId (const juce::String& id)
{
    return id.isNotEmpty() && id.containsOnly ("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._-")
           && ! id.startsWith (".");
}

bool isHexDigest (const juce::String& s)
{
    return s.length() == 64 && s.containsOnly ("0123456789abcdef");
}

std::optional<ModelManifest> failWith (juce::String* error, const juce::String& message)
{
    if (error != nullptr)
        *error = message;
    return std::nullopt;
}
} // namespace

std::optional<ModelManifest> ModelManifest::fromJson (const juce::String& json, juce::String* error)
{
    const auto v = juce::JSON::parse (json);
    if (! v.isObject())
        return failWith (error, "manifest is not a JSON object");

    ModelManifest m { v["id"].toString(), v["url"].toString(), v["sha256"].toString().toLowerCase(),
                      static_cast<juce::int64> (v["sizeBytes"]), v["licenseName"].toString(), v["licenseText"].toString() };
    if (! isSafeId (m.id))
        return failWith (error, "invalid model id");
    if (m.url.isEmpty())
        return failWith (error, "missing url");
    if (! isHexDigest (m.sha256))
        return failWith (error, "sha256 must be 64 hex digits");
    if (m.licenseName.isEmpty() || m.licenseText.isEmpty())
        return failWith (error, "licence name and text are required");
    return m;
}

juce::String ModelManifest::toJson() const
{
    auto obj = std::make_unique<juce::DynamicObject>();
    obj->setProperty ("id", id);
    obj->setProperty ("url", url);
    obj->setProperty ("sha256", sha256);
    obj->setProperty ("sizeBytes", sizeBytes);
    obj->setProperty ("licenseName", licenseName);
    obj->setProperty ("licenseText", licenseText);
    return juce::JSON::toString (juce::var (obj.release()), true);
}

} // namespace openmixer::stems
