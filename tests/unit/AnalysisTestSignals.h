// Synthetic test signals for analysis tests (no copyrighted audio): clicks, tones, WAV writing.
#pragma once

#include <JuceHeader.h>
#include "analysis/AudioClip.h"

#include <cmath>
#include <memory>

namespace openmixer::analysis::test
{

inline void writeWav (const juce::File& f, const juce::AudioBuffer<float>& buffer, double rate)
{
    f.deleteFile();
    juce::WavAudioFormat wav;
    std::unique_ptr<juce::FileOutputStream> stream (f.createOutputStream());
    std::unique_ptr<juce::AudioFormatWriter> writer (
        wav.createWriterFor (stream.get(), rate, static_cast<unsigned> (buffer.getNumChannels()), 24, {}, 0));
    if (writer == nullptr)
        return;
    stream.release();   // writer owns it now
    writer->writeFromAudioSampleBuffer (buffer, 0, buffer.getNumSamples());
}

inline AudioClip silence (double seconds, double rate = 44100.0)
{
    AudioClip clip;
    clip.sampleRate = rate;
    clip.samples.assign (static_cast<size_t> (seconds * rate), 0.0f);
    return clip;
}

// Adds a decaying noise-free click (damped 1 kHz burst) at the given time.
inline void addClick (AudioClip& clip, double timeSeconds, float gain = 0.8f, double freq = 1000.0)
{
    const auto start = static_cast<size_t> (timeSeconds * clip.sampleRate);
    const auto length = static_cast<size_t> (0.03 * clip.sampleRate);
    for (size_t i = 0; i < length && start + i < clip.samples.size(); ++i)
    {
        const auto t = static_cast<double> (i) / clip.sampleRate;
        clip.samples[start + i] += gain * static_cast<float> (std::exp (-t * 150.0) * std::sin (2.0 * juce::MathConstants<double>::pi * freq * t));
    }
}

inline void addTone (AudioClip& clip, double freq, float gain, double fromSeconds = 0.0, double toSeconds = 1.0e9)
{
    const auto from = static_cast<size_t> (fromSeconds * clip.sampleRate);
    const auto to = std::min (clip.samples.size(), static_cast<size_t> (toSeconds * clip.sampleRate));
    for (size_t i = from; i < to; ++i)
        clip.samples[i] += gain * static_cast<float> (std::sin (2.0 * juce::MathConstants<double>::pi * freq * static_cast<double> (i) / clip.sampleRate));
}

} // namespace openmixer::analysis::test
