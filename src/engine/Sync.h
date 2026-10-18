// Beat sync: matches a follower deck's tempo to a leader and nudges its phase onto the leader's
// beat (or bar). step()/apply() run on a control timer (~20-50 Hz); setters may come from any
// non-audio thread (MIDI). Rates reach the audio thread through Deck::setRate.
// Ids (ParameterTarget, e.g. "sync.<id>" via the router): <deck>.sync (>=0.5 on), leader (deck index,
// <0 none), mode (0 beat, 1 bar).
#pragma once

#include "../core/ParameterTarget.h"
#include "BeatGrid.h"

#include <memory>
#include <mutex>
#include <optional>
#include <vector>

namespace openmixer
{

class Deck;

enum class SyncMode { Beat, Bar };

struct SyncDeckState
{
    std::int64_t position = 0;   // source frames
    double rate = 1.0;
    bool playing = false;
};

class Sync : public ParameterTarget
{
public:
    static constexpr double kMaxNudge = 0.05;     // fraction of rate used to close phase error
    static constexpr double kPhaseGain = 0.25;    // nudge per beat of error
    static constexpr double kDeadband = 0.002;    // beats

    explicit Sync (int numDecks) : slots (static_cast<size_t> (numDecks)) {}

    // Grids are shared with the analysis side; nullptr removes the grid (deck cannot sync).
    void setGrid (int deck, std::shared_ptr<const BeatGrid> grid);
    void setLeader (int deck);
    void setEnabled (int deck, bool on);
    void setMode (SyncMode m);
    std::shared_ptr<const BeatGrid> getGrid (int deck) const;
    bool isEnabled (int deck) const;
    int getLeader() const;

    bool setParameter (const juce::String& id, double value) override;

    // New rate per deck, only for synced followers that need a change.
    std::vector<std::optional<double>> step (const std::vector<SyncDeckState>& decks) const;

    // Convenience for real decks (same order as the grids).
    void apply (const std::vector<Deck*>& decks) const;

    // Follower phase error vs leader in beats (bar mode: up to half a bar); 0 when not computable.
    double phaseError (const std::vector<SyncDeckState>& decks, int follower) const;

private:
    struct Slot { std::shared_ptr<const BeatGrid> grid; bool enabled = false; };

    // The helpers below expect the mutex to be held.
    bool validDeck (int deck) const noexcept { return deck >= 0 && deck < static_cast<int> (slots.size()); }
    bool canSync (const std::vector<SyncDeckState>& decks, int follower) const noexcept;
    double errorBeats (const std::vector<SyncDeckState>& decks, int follower) const noexcept;
    double phaseOf (const Slot& s, const SyncDeckState& d) const noexcept;
    std::optional<double> followerRate (const std::vector<SyncDeckState>& decks, int follower) const;

    mutable std::mutex mutex;   // guards everything below
    std::vector<Slot> slots;
    int leader = -1;
    SyncMode mode = SyncMode::Beat;
};

} // namespace openmixer
