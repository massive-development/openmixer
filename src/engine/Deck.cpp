#include "Deck.h"

#include "DeckDecoder.h"

#include <algorithm>

namespace openmixer
{

namespace
{
    constexpr auto kRelaxed = std::memory_order_relaxed;
    constexpr auto kRelease = std::memory_order_release;
}

//==============================================================================
Deck::Deck (size_t ringFrames, juce::int64 loopPrefetchFrames)
    : ring (ringFrames * 2), prefetch (loopPrefetchFrames)
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
    loopState.store ({});
    hotcues.clearAll();
    {
        const std::lock_guard<std::mutex> lock (pendingMutex);
        pendingReader = std::move (reader);
        pendingLoadId = ++loadCounter;
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
    loopState.store ({});
    hotcues.clearAll();
    {
        const std::lock_guard<std::mutex> lock (pendingMutex);
        pendingReader.reset();
        hasPendingReader = true;   // hands the decoder a null reader
    }
    requestReposition (0);
}

void Deck::seek (juce::int64 sourceFrame)
{
    const auto loop = getLoop();
    if (isLooping() && (sourceFrame < loop.start || sourceFrame >= loop.end))
        exitLoop();   // jumping out of a loop leaves it

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

bool Deck::setLoop (const Loop& loop) noexcept
{
    const auto len = getLengthFrames();
    const Loop clamped { std::clamp<juce::int64> (loop.start, 0, len), std::clamp<juce::int64> (loop.end, 0, len) };
    if (clamped.end - clamped.start < 2)
        return false;
    loopState.store ({ clamped.start, clamped.end, true });
    return true;
}

void Deck::setLoopEnabled (bool enabled) noexcept
{
    LoopSnapshot s;
    if (loopState.load (s) && s.valid())
        loopState.store ({ s.start, s.end, enabled });
}

bool Deck::isLooping() const noexcept
{
    LoopSnapshot s;
    return loopState.load (s) && s.active();
}

Loop Deck::getLoop() const noexcept
{
    LoopSnapshot s;
    return loopState.load (s) ? Loop { s.start, s.end } : Loop {};
}

void Deck::triggerHotcue (int index)
{
    if (const auto cue = hotcues.get (index))
        seek (*cue);
    else
        hotcues.set (index, getPositionFrames());
}

bool Deck::setParameter (const juce::String& id, double value)
{
    if (id == "play")             setPlaying (value >= 0.5);
    else if (id == "rate")        setRate (value);
    else if (id == "keylock")     setKeylock (value >= 0.5);
    else if (id == "seek")        seek (static_cast<juce::int64> (std::clamp (value, 0.0, 1.0) * static_cast<double> (getLengthFrames())));
    else if (id == "loop.exit")   { if (value >= 0.5) exitLoop(); }
    else if (id == "loop.reloop") { if (value >= 0.5) reloop(); }
    else return setHotcueParameter (id, value);
    return true;
}

bool Deck::setHotcueParameter (const juce::String& id, double value)
{
    if (! id.startsWith ("hotcue"))
        return false;
    const auto rest = id.substring (6);
    const auto digits = rest.upToFirstOccurrenceOf (".", false, false);
    const auto action = rest.fromFirstOccurrenceOf (".", false, false);
    const int index = digits.getIntValue();
    if (digits.isEmpty() || ! digits.containsOnly ("0123456789") || ! HotcueBank::inRange (index)
        || ! (action.isEmpty() || action == "clear"))
        return false;
    if (value >= 0.5)
        action.isEmpty() ? triggerHotcue (index) : clearHotcue (index);
    return true;
}

void Deck::prepare (double sr, int maxBlockSize)
{
    deviceSampleRate.store (sr > 0.0 ? sr : 44100.0, kRelaxed);
    monoSink.assign (static_cast<size_t> (std::max (1, maxBlockSize)), 0.0F);
    stretch.prepare (deviceSampleRate.load (kRelaxed), std::max (1, maxBlockSize));
    for (auto* buffers : { stretchIn, stretchSink })
        for (int c = 0; c < 2; ++c)
            buffers[c].assign (static_cast<size_t> (stretch.getMaxFeed()), 0.0F);
    stretchMode = StretchMode::off;
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

} // namespace openmixer
