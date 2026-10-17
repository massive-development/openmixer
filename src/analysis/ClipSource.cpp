#include "ClipSource.h"

#include <juce_cryptography/juce_cryptography.h>

namespace openmixer::analysis
{
namespace
{
constexpr int kReadBlock = 1 << 16;

void appendMono (const juce::AudioBuffer<float>& block, int frames, std::vector<float>& out)
{
    const auto channels = block.getNumChannels();
    const auto scale = 1.0f / static_cast<float> (channels);
    for (int i = 0; i < frames; ++i)
    {
        float sum = 0.0f;
        for (int c = 0; c < channels; ++c)
            sum += block.getSample (c, i);
        out.push_back (sum * scale);
    }
}

std::unique_ptr<juce::AudioFormatReader> openReader (const juce::File& file)
{
    juce::AudioFormatManager formats;
    formats.registerBasicFormats();
    return std::unique_ptr<juce::AudioFormatReader> (formats.createReaderFor (file));
}
} // namespace

juce::String hashFileContent (const juce::File& file)
{
    juce::FileInputStream in (file);
    if (! in.openedOk())
        return {};
    return juce::SHA256 (in).toHexString();
}

juce::String hashClip (const AudioClip& clip)
{
    juce::MemoryBlock bytes (clip.samples.data(), clip.samples.size() * sizeof (float));
    bytes.append (&clip.sampleRate, sizeof (clip.sampleRate));
    return juce::SHA256 (bytes).toHexString();
}

juce::String FileClipSource::getContentHash()
{
    return hashFileContent (file);
}

std::optional<AudioClip> FileClipSource::load (AnalysisContext& context)
{
    auto reader = openReader (file);
    if (reader == nullptr || reader->numChannels == 0 || reader->lengthInSamples <= 0)
        return std::nullopt;

    AudioClip clip;
    clip.sampleRate = reader->sampleRate;
    clip.samples.reserve (static_cast<size_t> (reader->lengthInSamples));

    juce::AudioBuffer<float> block (static_cast<int> (reader->numChannels), kReadBlock);
    for (juce::int64 pos = 0; pos < reader->lengthInSamples; pos += kReadBlock)
    {
        if (context.isCancelled())
            return std::nullopt;
        const auto frames = static_cast<int> (std::min<juce::int64> (kReadBlock, reader->lengthInSamples - pos));
        if (! reader->read (&block, 0, frames, pos, true, true))
            return std::nullopt;
        appendMono (block, frames, clip.samples);
        context.setProgress (static_cast<float> (pos + frames) / static_cast<float> (reader->lengthInSamples));
    }
    return clip;
}

juce::String MemoryClipSource::getContentHash()
{
    return hashClip (clip);
}

} // namespace openmixer::analysis
