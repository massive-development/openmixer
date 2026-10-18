// Applies a MidiMap to incoming events and forwards results to a ParameterTarget. Also the editing surface for the
// map: learn mode, binding CRUD, output (feedback) rules and save.
#pragma once

#include "../core/ParameterTarget.h"
#include "MidiLearn.h"
#include "MidiMap.h"

#include <atomic>
#include <functional>
#include <map>
#include <mutex>
#include <unordered_map>

namespace openmixer
{

class MidiMapper
{
public:
    // Thread-safe between control threads: a map reload (install) or edit is atomic with respect to handleMessage on
    // the MIDI thread. The lock is held across the target call; never call from the audio thread.
    void setTarget (ParameterTarget* t) noexcept { const std::lock_guard<std::recursive_mutex> guard (lock); target = t; }

    juce::Result loadFromJson (const juce::String& json);
    juce::Result loadFromFile (const juce::File& file);
    // Writes the current map (schema v2) to `file`; creates parent folders.
    juce::Result saveToFile (const juce::File& file) const;
    juce::String toJson() const;

    juce::String getName() const { const std::lock_guard<std::recursive_mutex> guard (lock); return map.name; }
    void setName (const juce::String& name) { const std::lock_guard<std::recursive_mutex> guard (lock); map.name = name; bump(); }
    size_t getNumMappings() const { const std::lock_guard<std::recursive_mutex> guard (lock); return map.mappings.size(); }

    // Returns the number of parameter updates produced.
    int handleMessage (const juce::MidiMessage& m);
    int handleEvent (const MidiEvent& e);

    // Last value produced for a target, or its initial value.
    double getValue (const juce::String& targetId) const;

    // Binding editing (any control thread). Snapshots are copies; indices refer to the order of getBindings().
    std::vector<Mapping> getBindings() const;
    juce::Result addBinding (const Mapping& m);                    // fails (map unchanged) for an invalid mapping
    juce::Result replaceBinding (size_t index, const Mapping& m);
    bool removeBinding (size_t index);
    int removeBindingsFor (const juce::String& targetId);   // returns how many were removed

    // Feedback rules.
    std::vector<OutputRule> getOutputs() const;
    void setOutputs (std::vector<OutputRule> rules);

    // Learn: the next CC/note/pitch bend binds to request.target (replacing the target's and the control's old
    // bindings) and is not forwarded. onLearned runs on the MIDI thread, outside the mapper lock: post to the message
    // thread before touching UI. Arm/cancel are safe from the message thread while MIDI arrives.
    void armLearn (const LearnRequest& request) { learn.arm (request); }
    void cancelLearn() { learn.cancel(); }
    bool isLearning() const noexcept { return learn.isArmed(); }
    void setOnLearned (std::function<void (const Mapping&)> callback);

    // Bumped by every load/edit; the feedback unit reloads its rules when it changes.
    unsigned getGeneration() const noexcept { return generation.load (std::memory_order_acquire); }

private:
    static int makeKey (MidiEvent::Kind k, int channel, int number) noexcept
    {
        return (static_cast<int> (k) << 16) | (channel << 8) | number;
    }

    void bump() noexcept { generation.fetch_add (1, std::memory_order_release); }
    void install (MidiMap&& newMap);
    void rebuildIndex();
    bool captureLearned (const MidiEvent& e);
    void bind (const Mapping& learned);
    double compute (const Mapping& m, const MidiEvent& e, double current, bool& changed) const noexcept;
    bool apply (const Mapping& m, const MidiEvent& e);
    void combine14 (const Mapping& m, MidiEvent& e);

    mutable std::recursive_mutex lock;
    ParameterTarget* target = nullptr;
    MidiMap map;
    std::unordered_map<int, std::vector<int>> index;
    std::map<juce::String, double> values;
    std::unordered_map<int, int> halves14;   // per (channel, MSB number): the 14-bit value so far
    MidiLearn learn;
    std::function<void (const Mapping&)> onLearned;
    std::atomic<unsigned> generation { 0 };
};

} // namespace openmixer
