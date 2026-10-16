#include "Deck.h"

#include "DeckDecoder.h"

#include <algorithm>

namespace openmixer
{

namespace
{
    constexpr auto kRelaxed = std::memory_order_relaxed;
    constexpr auto kAcquire = std::memory_order_acquire;
    constexpr auto kRelease = std::memory_order_release;
}

//==============================================================================
Deck::Deck (size_t ringFrames) : ring (ringFrames * 2)
{
    formatManager.registerBasicFormats();
    monoSink.assign (4096, 0.0F);
    decoder = std::make_unique<DeckDecoder> (*this);
    decoder->startThread();
}

Deck::~Deck()
{
    decoder->stopThread (2000);
}

bool Deck::load (const juce::File& file)
{
    std::unique_ptr<juce::AudioFormatReader> reader (formatManager.createReaderFor (file));
    if (reader == nullptr)
        return false;

    playing.store (false, kRelaxed);
    finished.store (false, kRelaxed);
    fileSampleRate.store (reader->sampleRate, kRelaxed);
    lengthFrames.store (reader->lengthInSamples, kRelaxed);
    {
        const std::lock_guard<std::mutex> lock (pendingMutex);
        pendingReader = std::move (reader);
        hasPendingReader = true;
    }
    loaded.store (true, kRelaxed);
    requestReposition (0);
    return true;
}

void Deck::unload()
{
    playing.store (false, kRelaxed);
    loaded.store (false, kRelaxed);
    lengthFrames.store (0, kRelaxed);
    {
        const std::lock_guard<std::mutex> lock (pendingMutex);
        pendingReader.reset();
        hasPendingReader = true;   // hands the decoder a null reader
    }
    requestReposition (0);
}

void Deck::seek (juce::int64 sourceFrame)
{
    finished.store (false, kRelaxed);
    requestReposition (std::max<juce::int64> (0, sourceFrame));
}

void Deck::requestReposition (juce::int64 target)
{
    seekTarget.store (target, kRelaxed);
    requestEpoch.fetch_add (1, kRelease);
    decoder->notify();
}

void Deck::setRate (double newRate) noexcept
{
    rate.store (std::clamp (newRate, 0.0, 8.0), kRelaxed);
}

bool Deck::setParameter (const juce::String& id, double value)
{
    if (id == "play")      setPlaying (value >= 0.5);
    else if (id == "rate") setRate (value);
    else if (id == "seek") seek (static_cast<juce::int64> (std::clamp (value, 0.0, 1.0) * static_cast<double> (getLengthFrames())));
    else return false;
    return true;
}

void Deck::prepare (double sr, int maxBlockSize)
{
    deviceSampleRate.store (sr > 0.0 ? sr : 44100.0, kRelaxed);
    monoSink.assign (static_cast<size_t> (std::max (1, maxBlockSize)), 0.0F);
}

void Deck::process (const float* const*, int, float* const* outputs, int numOutputs, int numSamples)
{
    if (numOutputs <= 0)
        return;

    if (numOutputs == 1)
    {
        const int cap = static_cast<int> (monoSink.size());
        for (int done = 0; done < numSamples; done += cap)
            renderStereo (outputs[0] + done, monoSink.data(), std::min (cap, numSamples - done));
        return;
    }

    renderStereo (outputs[0], outputs[1], numSamples);
    for (int c = 2; c < numOutputs; ++c)
        juce::FloatVectorOperations::clear (outputs[c], numSamples);
}

//==============================================================================
void Deck::clearTail (float* left, float* right, int from, int numSamples) const noexcept
{
    if (from >= numSamples)
        return;
    juce::FloatVectorOperations::clear (left + from, numSamples - from);
    juce::FloatVectorOperations::clear (right + from, numSamples - from);
}

bool Deck::serviceHandshake() noexcept
{
    const auto req = requestEpoch.load (kAcquire);
    const auto ack = ackEpoch.load (kAcquire);
    if (ack != req)
        return false;

    if (drainedEpoch.load (kRelaxed) != ack)
    {
        ring.skip (ring.getReadableCount());   // everything before the ack is stale
        pos = ackTarget.load (kRelaxed);
        primed = false;
        phase = 0.0;
        positionFrames.store (pos, kRelaxed);
        drainedEpoch.store (ack, kRelease);
    }
    return true;
}

bool Deck::reachedEnd() noexcept
{
    // Re-check readability after seeing end-of-stream: the producer's last write may race.
    if (endOfStream.load (kAcquire) && ring.getReadableCount() < 2)
    {
        playing.store (false, kRelaxed);
        finished.store (true, kRelaxed);
        return true;
    }
    return false;
}

void Deck::noteStarved() noexcept
{
    if (! reachedEnd())
        underruns.fetch_add (1, kRelaxed);
}

bool Deck::popFrame() noexcept
{
    float f[2];
    if (ring.read (f, 2) < 2)
        return false;
    prev[0] = next[0]; prev[1] = next[1];
    next[0] = f[0];    next[1] = f[1];
    return true;
}

bool Deck::primeInterpolator() noexcept
{
    float two[4];
    if (ring.read (two, 4) < 4)
        return false;
    prev[0] = two[0]; prev[1] = two[1];
    next[0] = two[2]; next[1] = two[3];
    primed = true;
    phase = 0.0;
    return true;
}

void Deck::renderStereo (float* left, float* right, int numSamples) noexcept
{
    const bool running = serviceHandshake() && playing.load (kRelaxed) && loaded.load (kRelaxed);
    if (! running)
    {
        clearTail (left, right, 0, numSamples);
        return;
    }

    if (! primed && ring.getReadableCount() < 4)
    {
        noteStarved();
        clearTail (left, right, 0, numSamples);
        return;
    }
    if (! primed)
        primeInterpolator();

    const double ratio = rate.load (kRelaxed) * (fileSampleRate.load (kRelaxed) / deviceSampleRate.load (kRelaxed));
    for (int i = 0; i < numSamples; ++i)
    {
        while (phase >= 1.0)
        {
            if (! popFrame())
            {
                noteStarved();
                clearTail (left, right, i, numSamples);
                positionFrames.store (pos, kRelaxed);
                return;
            }
            phase -= 1.0;
            ++pos;
        }
        const auto t = static_cast<float> (phase);
        left[i]  = prev[0] + (next[0] - prev[0]) * t;
        right[i] = prev[1] + (next[1] - prev[1]) * t;
        phase += ratio;
    }
    positionFrames.store (pos, kRelaxed);
}

} // namespace openmixer
