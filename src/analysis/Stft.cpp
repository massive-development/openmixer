#include "Stft.h"

#include <cmath>
#include <numeric>

namespace openmixer::analysis
{

MagnitudeStft::MagnitudeStft (int fftOrder, int hopSize)
    : frameSize (1 << fftOrder), hop (std::max (1, hopSize)), fft (fftOrder),
      window (static_cast<size_t> (frameSize)), scratch (static_cast<size_t> (frameSize) * 2)
{
    for (int i = 0; i < frameSize; ++i)
        window[static_cast<size_t> (i)] = 0.5f - 0.5f * std::cos (2.0f * juce::MathConstants<float>::pi
                                                                   * static_cast<float> (i) / static_cast<float> (frameSize));
    scale = 2.0f / std::accumulate (window.begin(), window.end(), 0.0f);
}

int MagnitudeStft::getNumFrames (size_t numSamples) const noexcept
{
    if (numSamples == 0)
        return 0;
    const auto n = static_cast<long long> (numSamples);
    return n <= frameSize ? 1 : static_cast<int> (1 + (n - frameSize + hop - 1) / hop);
}

double MagnitudeStft::getBinHz (int bin, double sampleRate) const noexcept
{
    return static_cast<double> (bin) * sampleRate / static_cast<double> (frameSize);
}

void MagnitudeStft::computeFrame (const std::vector<float>& samples, int index, std::vector<float>& magnitudes)
{
    std::fill (scratch.begin(), scratch.end(), 0.0f);
    const auto start = static_cast<size_t> (index) * static_cast<size_t> (hop);
    for (size_t i = 0; i < static_cast<size_t> (frameSize) && start + i < samples.size(); ++i)
        scratch[i] = samples[start + i] * window[i];

    fft.performFrequencyOnlyForwardTransform (scratch.data(), true);
    magnitudes.resize (static_cast<size_t> (getNumBins()));
    for (size_t k = 0; k < magnitudes.size(); ++k)
        magnitudes[k] = scratch[k] * scale;
}

} // namespace openmixer::analysis
