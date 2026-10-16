#include "Deck.h"

#include <algorithm>
#include <cstring>

namespace openmixer
{

//==============================================================================
class Deck::DecoderThread : public juce::Thread
{
public:
    explicit DecoderThread (Deck& d) : juce::Thread ("OpenMixer Deck Decoder"), deck (d) {}

    void run() override
    {
        constexpr int chunk = 2048;
        juce::AudioBuffer<float> tmp (2, chunk);
        std::vector<float> interleaved ((size_t) chunk * 2);

        std::unique_ptr<juce::AudioFormatReader> reader;
        juce::int64 readPos = 0;
        juce::uint32 localAck = 0;

        while (! threadShouldExit())
        {
            // Read the epoch first, then the pending reader: a reader is always
            // published before the epoch bump that refers to it.
            const auto epoch = deck.requestEpoch.load (std::memory_order_acquire);
            {
                std::lock_guard<std::mutex> lock (deck.pendingMutex);
                if (deck.hasPendingReader)
                {
                    reader = std::move (deck.pendingReader);
                    deck.hasPendingReader = false;
                }
            }

            if (epoch != localAck)
            {
                const auto len = reader != nullptr ? reader->lengthInSamples : 0;
                readPos = std::clamp<juce::int64> (deck.seekTarget.load(), 0, len);
                deck.ackTarget.store (readPos);
                deck.endOfStream.store (reader == nullptr || readPos >= len);
                localAck = epoch;
                deck.ackEpoch.store (epoch, std::memory_order_release);
            }

            if (deck.drainedEpoch.load (std::memory_order_acquire) != localAck
                || reader == nullptr
                || deck.ring.getWritableCount() < (size_t) chunk * 2)
            {
                wait (3);
                continue;
            }

            if (readPos >= reader->lengthInSamples)
            {
                deck.endOfStream.store (true, std::memory_order_release);
                wait (10);
                continue;
            }

            const int n = (int) std::min<juce::int64> (chunk, reader->lengthInSamples - readPos);
            const int fileChans = (int) std::min<unsigned int> (reader->numChannels, 2u);
            tmp.clear();
            reader->read (tmp.getArrayOfWritePointers(), fileChans, readPos, n);
            if (fileChans == 1)
                tmp.copyFrom (1, 0, tmp, 0, 0, n);

            const float* l = tmp.getReadPointer (0);
            const float* r = tmp.getReadPointer (1);
            for (int i = 0; i < n; ++i)
            {
                interleaved[(size_t) i * 2] = l[i];
                interleaved[(size_t) i * 2 + 1] = r[i];
            }
            deck.ring.write (interleaved.data(), (size_t) n * 2);
            readPos += n;
            if (readPos >= reader->lengthInSamples)
                deck.endOfStream.store (true, std::memory_order_release);
        }
    }

private:
    Deck& deck;
};

//==============================================================================
Deck::Deck (size_t ringFrames) : ring (ringFrames * 2)
{
    formatManager.registerBasicFormats();
    decoder = std::make_unique<DecoderThread> (*this);
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

    playing.store (false);
    finished.store (false);
    fileSampleRate.store (reader->sampleRate);
    lengthFrames.store (reader->lengthInSamples);
    {
        std::lock_guard<std::mutex> lock (pendingMutex);
        pendingReader = std::move (reader);
        hasPendingReader = true;
    }
    loaded.store (true);
    requestReposition (0);
    return true;
}

void Deck::unload()
{
    playing.store (false);
    loaded.store (false);
    lengthFrames.store (0);
    {
        std::lock_guard<std::mutex> lock (pendingMutex);
        pendingReader.reset();
        hasPendingReader = true;   // hand the decoder a null reader
    }
    requestReposition (0);
}

void Deck::seek (juce::int64 sourceFrame)
{
    finished.store (false);
    requestReposition (std::max<juce::int64> (0, sourceFrame));
}

void Deck::requestReposition (juce::int64 target)
{
    seekTarget.store (target);
    requestEpoch.fetch_add (1, std::memory_order_release);
    decoder->notify();
}

void Deck::setRate (double newRate) noexcept
{
    rate.store (std::clamp (newRate, 0.0, 8.0));
}

void Deck::prepare (double sr, int)
{
    deviceSampleRate.store (sr > 0.0 ? sr : 44100.0);
}

void Deck::process (const float* const*, int, float* const* outputs, int numOutputs, int numSamples)
{
    if (numOutputs <= 0)
        return;
    if (numOutputs == 1)
    {
        // Mono device: render into the left channel only (right is a dummy sink).
        static thread_local float dummy[8192];
        for (int done = 0; done < numSamples;)
        {
            const int n = std::min(numSamples - done, 8192);
            renderStereo (outputs[0] + done, dummy, n);
            done += n;
        }
        return;
    }
    renderStereo (outputs[0], outputs[1], numSamples);
    for (int c = 2; c < numOutputs; ++c)
        juce::FloatVectorOperations::clear (outputs[c], numSamples);
}

void Deck::renderStereo (float* left, float* right, int numSamples) noexcept
{
    auto silence = [&] (int from)
    {
        if (from < numSamples)
        {
            juce::FloatVectorOperations::clear (left + from, numSamples - from);
            juce::FloatVectorOperations::clear (right + from, numSamples - from);
        }
    };

    // Epoch handshake (see header).
    const auto req = requestEpoch.load (std::memory_order_acquire);
    const auto ack = ackEpoch.load (std::memory_order_acquire);
    if (ack != req)
    {
        silence (0);
        return;
    }
    if (drainedEpoch.load (std::memory_order_relaxed) != ack)
    {
        ring.skip (ring.getReadableCount());     // stale data from before the ack
        pos = ackTarget.load();
        primed = false;
        phase = 0.0;
        positionFrames.store (pos);
        drainedEpoch.store (ack, std::memory_order_release);
    }

    if (! playing.load (std::memory_order_relaxed) || ! loaded.load (std::memory_order_relaxed))
    {
        silence (0);
        return;
    }

    auto reachedEnd = [&]
    {
        // Re-check readability after observing end-of-stream to avoid a race
        // with the producer's final write.
        if (endOfStream.load (std::memory_order_acquire) && ring.getReadableCount() < 2)
        {
            playing.store (false);
            finished.store (true);
            return true;
        }
        return false;
    };

    if (! primed)
    {
        float two[4];
        if (ring.getReadableCount() < 4)
        {
            if (! reachedEnd())
                underruns.fetch_add (1, std::memory_order_relaxed);
            silence (0);
            return;
        }
        ring.read (two, 4);
        prev[0] = two[0]; prev[1] = two[1];
        next[0] = two[2]; next[1] = two[3];
        primed = true;
        phase = 0.0;
    }

    const double ratio = rate.load (std::memory_order_relaxed)
                         * (fileSampleRate.load (std::memory_order_relaxed) / deviceSampleRate.load (std::memory_order_relaxed));

    for (int i = 0; i < numSamples; ++i)
    {
        while (phase >= 1.0)
        {
            float f[2];
            if (ring.read (f, 2) < 2)
            {
                if (! reachedEnd())
                    underruns.fetch_add (1, std::memory_order_relaxed);
                silence (i);
                positionFrames.store (pos, std::memory_order_relaxed);
                return;
            }
            prev[0] = next[0]; prev[1] = next[1];
            next[0] = f[0]; next[1] = f[1];
            phase -= 1.0;
            ++pos;
        }
        const float t = (float) phase;
        left[i]  = prev[0] + (next[0] - prev[0]) * t;
        right[i] = prev[1] + (next[1] - prev[1]) * t;
        phase += ratio;
    }
    positionFrames.store (pos, std::memory_order_relaxed);
}

} // namespace openmixer
