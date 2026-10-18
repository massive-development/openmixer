#include "WaveformData.h"

#include <algorithm>
#include <cmath>

namespace openmixer::analysis
{
namespace
{
constexpr size_t kBytesPerPoint = 5;

WaveformPoint mergePoints (const WaveformPoint* first, size_t count)
{
    WaveformPoint out { 1.0f, -1.0f, 0.0f, 0.0f, 0.0f };
    double low = 0.0, mid = 0.0, high = 0.0;
    for (size_t i = 0; i < count; ++i)
    {
        const auto& p = first[i];
        out.min = std::min (out.min, p.min);
        out.max = std::max (out.max, p.max);
        low += p.low * p.low; mid += p.mid * p.mid; high += p.high * p.high;
    }
    const auto n = static_cast<double> (count);
    out.low = static_cast<float> (std::sqrt (low / n));
    out.mid = static_cast<float> (std::sqrt (mid / n));
    out.high = static_cast<float> (std::sqrt (high / n));
    return out;
}

WaveformLevel coarsen (const WaveformLevel& fine, int factor)
{
    WaveformLevel out { fine.samplesPerPoint * factor, {} };
    const auto step = static_cast<size_t> (factor);
    for (size_t i = 0; i < fine.points.size(); i += step)
        out.points.push_back (mergePoints (fine.points.data() + i, std::min (step, fine.points.size() - i)));
    return out;
}

// Ranges use the full byte; band RMS is stored as sqrt for more resolution at low levels.
juce::uint8 quantiseSigned (float v) { return static_cast<juce::uint8> (std::lround ((juce::jlimit (-1.0f, 1.0f, v) + 1.0f) * 127.5f)); }
juce::uint8 quantiseLevel (float v) { return static_cast<juce::uint8> (std::lround (std::sqrt (juce::jlimit (0.0f, 1.0f, v)) * 255.0f)); }
float decodeSigned (juce::uint8 q) { return static_cast<float> (q) / 127.5f - 1.0f; }
float decodeLevel (juce::uint8 q) { const auto r = static_cast<float> (q) / 255.0f; return r * r; }

juce::MemoryBlock encodePoints (const std::vector<WaveformPoint>& points)
{
    juce::MemoryBlock block (points.size() * kBytesPerPoint);
    auto* bytes = static_cast<juce::uint8*> (block.getData());
    for (const auto& p : points)
    {
        *bytes++ = quantiseSigned (p.min);
        *bytes++ = quantiseSigned (p.max);
        *bytes++ = quantiseLevel (p.low);
        *bytes++ = quantiseLevel (p.mid);
        *bytes++ = quantiseLevel (p.high);
    }
    return block;
}

std::vector<WaveformPoint> decodePoints (const juce::MemoryBlock& block)
{
    std::vector<WaveformPoint> points (block.getSize() / kBytesPerPoint);
    const auto* bytes = static_cast<const juce::uint8*> (block.getData());
    for (auto& p : points)
    {
        p = { decodeSigned (bytes[0]), decodeSigned (bytes[1]), decodeLevel (bytes[2]), decodeLevel (bytes[3]), decodeLevel (bytes[4]) };
        bytes += kBytesPerPoint;
    }
    return points;
}
} // namespace

WaveformData::WaveformData (double sampleRate, juce::int64 numSamples, WaveformLevel base)
    : rate (sampleRate), length (numSamples)
{
    if (base.samplesPerPoint <= 0 || base.points.empty())
        return;
    levels.push_back (std::move (base));
    buildCoarserLevels();
}

void WaveformData::buildCoarserLevels()
{
    while (levels.back().points.size() > static_cast<size_t> (kOverviewPoints))
        levels.push_back (coarsen (levels.back(), kLevelFactor));
}

const WaveformLevel& WaveformData::levelFor (double samplesPerPixel) const noexcept
{
    static const WaveformLevel empty;
    if (levels.empty())
        return empty;
    for (auto it = levels.rbegin(); it != levels.rend(); ++it)
        if (static_cast<double> (it->samplesPerPoint) <= samplesPerPixel)
            return *it;
    return levels.front();
}

juce::String WaveformData::toJson() const
{
    auto obj = std::make_unique<juce::DynamicObject>();
    obj->setProperty ("version", kFormatVersion);
    obj->setProperty ("sampleRate", rate);
    obj->setProperty ("numSamples", length);
    if (isValid())
    {
        obj->setProperty ("samplesPerPoint", levels.front().samplesPerPoint);
        obj->setProperty ("points", encodePoints (levels.front().points).toBase64Encoding());
    }
    return juce::JSON::toString (juce::var (obj.release()), true);
}

std::optional<WaveformData> WaveformData::fromJson (const juce::String& json)
{
    const auto v = juce::JSON::parse (json);
    if (static_cast<int> (v["version"]) != kFormatVersion)
        return std::nullopt;
    if (! v.hasProperty ("points"))
        return WaveformData {};   // valid payload for an empty clip

    juce::MemoryBlock block;
    if (! block.fromBase64Encoding (v["points"].toString()) || block.getSize() % kBytesPerPoint != 0)
        return std::nullopt;
    WaveformData data (static_cast<double> (v["sampleRate"]), static_cast<juce::int64> (v["numSamples"]),
                       { static_cast<int> (v["samplesPerPoint"]), decodePoints (block) });
    return data.isValid() ? std::optional<WaveformData> (std::move (data)) : std::nullopt;
}

} // namespace openmixer::analysis
