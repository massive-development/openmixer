// Controller feedback: evaluates the map's output rules against engine state and sends a note/CC only when the
// value changes (blinking rules change on a half-period clock). Message thread only; reads engine state through a
// StateReader (lock-free atomics), never touches the audio thread.
#pragma once

#include "../core/StateReader.h"
#include "MidiMapper.h"
#include "MidiOutputSink.h"

#include <juce_events/juce_events.h>

#include <functional>
#include <optional>
#include <vector>

namespace openmixer
{

class MidiFeedback : private juce::Timer
{
public:
    using Clock = std::function<double()>;   // milliseconds, monotonic

    static constexpr int kDefaultHz = 30;

    // `clock` defaults to the high-resolution millisecond counter; tests inject their own.
    MidiFeedback (const StateReader& state, MidiMapper& mapper, Clock clock = {});
    ~MidiFeedback() override { stopTimer(); }

    // nullptr stops sending. A new sink gets every LED re-sent on the next update.
    void setSink (MidiOutputSink* newSink);
    void start (int hz = kDefaultHz) { startTimerHz (hz); }
    void stop() { stopTimer(); }

    // One evaluation pass at the injected clock's time; returns how many messages were sent.
    int update();
    // Forget what was sent: the next update re-sends every LED (controller reconnected, map reloaded).
    void resend() { lastSent.assign (rules.size(), std::nullopt); }

private:
    void timerCallback() override { update(); }
    void reloadRules();

    const StateReader& state;
    MidiMapper& mapper;
    Clock clock;
    MidiOutputSink* sink = nullptr;
    unsigned seenGeneration = 0;
    bool haveGeneration = false;
    std::vector<OutputRule> rules;
    std::vector<std::optional<int>> lastSent;
};

} // namespace openmixer
