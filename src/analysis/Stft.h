// Hann-windowed magnitude STFT over a mono clip. Holds scratch buffers, so use one instance per
// thread. Frame i covers samples [i * hop, i * hop + frameSize); the tail is zero padded.
#pragma once

#include <juce_dsp/juce_dsp.h>

#include <vector>

namespace openmixer::analysis
{

class MagnitudeStft
{
public:
    MagnitudeStft (int fftOrder, int hopSize);

    int getFrameSize() const noexcept { return frameSize; }
    int getHop() const noexcept { return hop; }
    int getNumBins() const noexcept { return frameSize / 2 + 1; }
    int getNumFrames (size_t numSamples) const noexcept;
    double getBinHz (int bin, double sampleRate) const noexcept;

    // Magnitudes of frame `index` (getNumBins() values, normalised so a full-scale sine peaks near 1).
    void computeFrame (const std::vector<float>& samples, int index, std::vector<float>& magnitudes);

private:
    int frameSize, hop;
    juce::dsp::FFT fft;
    std::vector<float> window, scratch;
    float scale = 1.0f;
};

} // namespace openmixer::analysis
