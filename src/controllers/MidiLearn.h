// MIDI learn: arm a target id, the next CC/note/pitch bend becomes a Mapping for it.
// Thread-safe: arm/cancel come from the message thread, capture from the MIDI thread (own mutex, tiny critical
// sections, an atomic flag keeps the not-armed path lock-free). The mapper owns one and installs the result.
#pragma once

#include "MidiMap.h"

#include <atomic>
#include <mutex>
#include <optional>

namespace openmixer
{

// What the learned control is. Auto: notes become buttons, CCs and pitch bend absolute.
enum class LearnType { Auto, Absolute, Relative, RelativeTwosComplement, Button, Toggle, Absolute14 };

struct LearnRequest
{
    juce::String target;
    LearnType type = LearnType::Auto;
    double min = 0.0, max = 1.0;
    double step = -1.0;       // < 0: the mapping default, (max - min) / 127
    double initial = 0.0;
    bool anyChannel = false;  // false: bind to the channel the control sent on
    bool invert = false;

    // Sensible type and range for a router id (eq -48..6 dB, filter -1..1, rate 0.92..1.08, buttons for pfl/play/...).
    static LearnRequest forTarget (const juce::String& targetId);
};

class MidiLearn
{
public:
    void arm (const LearnRequest& request);
    void cancel();
    bool isArmed() const noexcept { return armed.load (std::memory_order_acquire); }
    LearnRequest getRequest() const;

    // MIDI thread: returns the new mapping and disarms, or nothing (not armed, or an event that cannot bind,
    // such as a note-off). The event is consumed whenever a mapping is returned.
    std::optional<Mapping> capture (const MidiEvent& e);

private:
    mutable std::mutex lock;
    LearnRequest request;
    std::atomic<bool> armed { false };
};

} // namespace openmixer
