#include "OutputRouter.h"

#include <algorithm>

namespace openmixer
{

void OutputRouter::prepare (int maxBlockSize)
{
    scratch.setSize (4, std::max (1, maxBlockSize));
}

void OutputRouter::process (AudioCallback& source, float* const* deviceOut, int numDeviceOut, int numSamples)
{
    if (numDeviceOut < 2)   // mono device: the engine folds down itself
    {
        source.process (nullptr, 0, deviceOut, numDeviceOut, numSamples);
        return;
    }
    const int chunk = scratch.getNumSamples();
    for (int done = 0; done < numSamples; done += chunk)
    {
        const int n = std::min (chunk, numSamples - done);
        float* engineOut[4] = { scratch.getWritePointer (0), scratch.getWritePointer (1),
                                scratch.getWritePointer (2), scratch.getWritePointer (3) };
        source.process (nullptr, 0, engineOut, 4, n);
        route (deviceOut, numDeviceOut, done, n);
    }
}

// An out-of-range pair is treated as unrouted so a stale preference can never write past the device.
void OutputRouter::route (float* const* deviceOut, int numDeviceOut, int offset, int n) noexcept
{
    for (int c = 0; c < numDeviceOut; ++c)
        juce::FloatVectorOperations::clear (deviceOut[c] + offset, n);
    const auto copyPair = [&] (int pair, int firstEngineChannel)
    {
        if (pair < 0 || pair * 2 + 1 >= numDeviceOut)
            return;
        for (int side = 0; side < 2; ++side)
            juce::FloatVectorOperations::copy (deviceOut[pair * 2 + side] + offset,
                                               scratch.getReadPointer (firstEngineChannel + side), n);
    };
    copyPair (getMasterPair(), 0);
    if (getCuePair() != getMasterPair())
        copyPair (getCuePair(), 2);
}

} // namespace openmixer
