#include "DeckControl.h"

#include <algorithm>
#include <cmath>

namespace openmixer
{

namespace
{
    constexpr auto kLoopPrefix = "loop.quantized.";
}

bool DeckControl::setSyncParameter (const juce::String& id, double value)
{
    if (id == "sync")
        sync.setEnabled (index, value >= 0.5);
    else if (id == "sync.master")
        setLeader (value >= 0.5);
    else
        return false;
    return true;
}

void DeckControl::setLeader (bool on)
{
    if (on)
        sync.setLeader (index);
    else if (sync.getLeader() == index)
        sync.setLeader (-1);   // only the current leader can be released
}

bool DeckControl::setLoopParameter (const juce::String& id, double value)
{
    const auto text = id.fromFirstOccurrenceOf (kLoopPrefix, false, false);
    const double beats = text.getDoubleValue();
    const bool wellFormed = text.isNotEmpty() && text.containsOnly ("0123456789.") && text.indexOfChar ('.') == text.lastIndexOfChar ('.')
                            && ! text.startsWithChar ('.') && ! text.endsWithChar ('.') && std::isfinite (beats) && beats > 0.0;
    if (! wellFormed)
        return false;
    return value < 0.5 || setQuantizedLoop (beats);   // release is a no-op; press reports whether a loop was set
}

bool DeckControl::setParameter (const juce::String& id, double value)
{
    if (setSyncParameter (id, value))
        return true;
    if (id.startsWith (kLoopPrefix))
        return setLoopParameter (id, value);
    if (id == "slip")
    {
        setSlip (value >= 0.5);
        return true;
    }
    return deck.setParameter (id, value);
}

bool DeckControl::setQuantizedLoop (double lengthBeats)
{
    const auto grid = sync.getGrid (index);
    // Snap the start to the loop's own length, but never coarser than a beat.
    const auto loop = makeQuantizedLoop (grid.get(), deck.getPositionFrames(), lengthBeats, std::min (lengthBeats, 1.0));
    return loop.valid() && deck.setLoop (loop);
}

void DeckControl::setSlip (bool on)
{
    std::int64_t resume = -1;
    {
        const std::scoped_lock lock (slipMutex);
        if (on)
            slip.begin (deck.getPositionFrames());
        else
            resume = slip.end (deck.getLengthFrames());
    }
    if (resume >= 0)
    {
        deck.exitLoop();
        deck.seek (resume);
    }
}

bool DeckControl::isSlipping() const
{
    const std::scoped_lock lock (slipMutex);
    return slip.isActive();
}

void DeckControl::tick (double elapsedSeconds)
{
    if (! deck.isPlaying())
        return;
    const std::scoped_lock lock (slipMutex);
    slip.advance (elapsedSeconds * deck.getRate() * deck.getFileSampleRate());
}

} // namespace openmixer
