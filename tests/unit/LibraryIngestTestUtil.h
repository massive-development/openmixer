// Shared fixtures for the ingest tests: temp folders and generated WAV files.
#pragma once

#include <juce_audio_formats/juce_audio_formats.h>

namespace openmixer::library::test
{

class TempDir
{
public:
    TempDir()
        : dir (juce::File::getSpecialLocation (juce::File::tempDirectory)
                   .getNonexistentChildFile ("om_ingest", "", false))
    {
        dir.createDirectory();
    }
    ~TempDir() { dir.deleteRecursively(); }

    juce::File dir;
};

// Short sine WAV with optional RIFF INFO tags; `seconds` sets the duration, `freq` the content.
inline juce::File writeWav (const juce::File& f, double seconds, const juce::StringPairArray& tags = {},
                            float freq = 440.0f)
{
    f.getParentDirectory().createDirectory();
    f.deleteFile();
    const double rate = 8000.0;
    const auto n = static_cast<int> (seconds * rate);
    juce::AudioBuffer<float> buffer (1, n);
    for (int i = 0; i < n; ++i)
        buffer.setSample (0, i, 0.5f * std::sin (6.2831853f * freq * static_cast<float> (i) / static_cast<float> (rate)));

    juce::WavAudioFormat wav;
    std::unique_ptr<juce::FileOutputStream> stream (f.createOutputStream());
    std::unique_ptr<juce::AudioFormatWriter> writer (wav.createWriterFor (stream.get(), rate, 1, 16, tags, 0));
    if (writer != nullptr)
    {
        stream.release();   // writer owns it now
        writer->writeFromAudioSampleBuffer (buffer, 0, n);
    }
    return f;
}

} // namespace openmixer::library::test
