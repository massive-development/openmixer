#include "DjEngine.h"

#include <algorithm>

namespace openmixer
{

DjEngine::DjEngine (int numDecks) : mixer (std::min (numDecks, Mixer::kMaxChannels))
{
    for (int i = 0; i < mixer.getNumChannels(); ++i)
        decks.push_back (std::make_unique<Deck>());
}

void DjEngine::prepare (double sampleRate, int maxBlockSize)
{
    maxBlock = std::max (1, maxBlockSize);
    for (auto& d : decks)
        d->prepare (sampleRate, maxBlock);
    mixer.prepare (sampleRate, maxBlock);
    deckBuffers.assign (decks.size() * 2, std::vector<float> (static_cast<size_t> (maxBlock), 0.0F));
    for (auto* sink : { &masterSink[0], &masterSink[1], &cueSink[0], &cueSink[1] })
        sink->assign (static_cast<size_t> (maxBlock), 0.0F);
}

void DjEngine::process (const float* const*, int, float* const* outputs, int numOutputs, int numSamples)
{
    for (int done = 0; done < numSamples; done += maxBlock)
        renderBlock (outputs, numOutputs, done, std::min (maxBlock, numSamples - done));
}

void DjEngine::renderBlock (float* const* outputs, int numOutputs, int offset, int n) noexcept
{
    Mixer::ChannelInput inputs[Mixer::kMaxChannels];
    for (size_t d = 0; d < decks.size(); ++d)
    {
        float* l = deckBuffers[d * 2].data();
        float* r = deckBuffers[d * 2 + 1].data();
        decks[d]->renderStereo (l, r, n);
        inputs[d] = { l, r };
    }

    const bool hasMaster = numOutputs >= 2, hasCue = numOutputs >= 4;
    float* master[2] { hasMaster ? outputs[0] + offset : masterSink[0].data(), hasMaster ? outputs[1] + offset : masterSink[1].data() };
    float* cue[2] { hasCue ? outputs[2] + offset : cueSink[0].data(), hasCue ? outputs[3] + offset : cueSink[1].data() };
    mixer.process (inputs, master[0], master[1], cue[0], cue[1], n);

    for (int c = hasCue ? 4 : 2; c < numOutputs; ++c)
        std::fill (outputs[c] + offset, outputs[c] + offset + n, 0.0F);
    if (numOutputs == 1)
        std::copy (masterSink[0].begin(), masterSink[0].begin() + n, outputs[0] + offset);
}

} // namespace openmixer
