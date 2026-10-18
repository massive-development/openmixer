// Generates the soak/UI-gate test tracks: a sine plus light noise, optionally with a click on every beat.
#pragma once

#include <juce_audio_formats/juce_audio_formats.h>

#include <cmath>
#include <memory>

namespace openmixer::soak
{

struct ToneSpec
{
    double hz = 220.0;
    int seconds = 6;
    double sampleRate = 48000.0;
    double clickBpm = 0.0;   // > 0 adds a short click per beat so the BPM analyser has a pulse to find
};

// Returns an empty File when the file cannot be written.
inline juce::File writeTone (const juce::File& file, const ToneSpec& spec)
{
    const int frames = static_cast<int> (spec.sampleRate) * spec.seconds;
    const double framesPerBeat = spec.clickBpm > 0.0 ? spec.sampleRate * 60.0 / spec.clickBpm : 0.0;
    juce::AudioBuffer<float> buf (2, frames);
    juce::Random noise (7);
    for (int i = 0; i < frames; ++i)
    {
        auto v = 0.4F * std::sin (juce::MathConstants<float>::twoPi * static_cast<float> (spec.hz * i / spec.sampleRate));
        if (framesPerBeat > 0.0)
        {
            const auto sinceBeat = std::fmod (static_cast<double> (i), framesPerBeat);
            v += sinceBeat < 400.0 ? 0.5F * static_cast<float> (1.0 - sinceBeat / 400.0) : 0.0F;   // decaying click
        }
        for (int c = 0; c < 2; ++c)
            buf.setSample (c, i, v + 0.1F * (noise.nextFloat() - 0.5F));
    }

    file.deleteFile();
    juce::WavAudioFormat wav;
    std::unique_ptr<juce::FileOutputStream> stream (file.createOutputStream());
    if (stream == nullptr)
        return {};
    std::unique_ptr<juce::AudioFormatWriter> writer (wav.createWriterFor (stream.get(), spec.sampleRate, 2, 24, {}, 0));
    if (writer == nullptr)
        return {};
    stream.release();   // the writer owns the stream
    writer->writeFromAudioSampleBuffer (buf, 0, frames);
    return file;
}

} // namespace openmixer::soak
