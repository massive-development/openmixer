#include "Sync.h"

#include "Deck.h"

#include <algorithm>
#include <cmath>

namespace openmixer
{

namespace
{
    double wrapHalf (double x) noexcept
    {
        return x - std::floor (x + 0.5);   // [-0.5, 0.5)
    }
}

void Sync::setGrid (int deck, std::shared_ptr<const BeatGrid> grid)
{
    const std::scoped_lock lock (mutex);
    if (validDeck (deck))
        slots[static_cast<size_t> (deck)].grid = std::move (grid);
}

void Sync::setEnabled (int deck, bool on)
{
    const std::scoped_lock lock (mutex);
    if (validDeck (deck))
        slots[static_cast<size_t> (deck)].enabled = on;
}

void Sync::setLeader (int deck)
{
    const std::scoped_lock lock (mutex);
    leader = validDeck (deck) ? deck : -1;
}

void Sync::setMode (SyncMode m)
{
    const std::scoped_lock lock (mutex);
    mode = m;
}

std::shared_ptr<const BeatGrid> Sync::getGrid (int deck) const
{
    const std::scoped_lock lock (mutex);
    return validDeck (deck) ? slots[static_cast<size_t> (deck)].grid : nullptr;
}

bool Sync::isEnabled (int deck) const
{
    const std::scoped_lock lock (mutex);
    return validDeck (deck) && slots[static_cast<size_t> (deck)].enabled;
}

int Sync::getLeader() const
{
    const std::scoped_lock lock (mutex);
    return leader;
}

bool Sync::setParameter (const juce::String& id, double value)
{
    if (id == "leader")
        setLeader (static_cast<int> (value));
    else if (id == "mode")
        setMode (value >= 0.5 ? SyncMode::Bar : SyncMode::Beat);
    else if (id.endsWith (".sync") && id.upToFirstOccurrenceOf (".", false, false).containsOnly ("0123456789"))
        setEnabled (id.upToFirstOccurrenceOf (".", false, false).getIntValue(), value >= 0.5);
    else
        return false;
    return true;
}

bool Sync::canSync (const std::vector<SyncDeckState>& decks, int follower) const noexcept
{
    const auto count = std::min (decks.size(), slots.size());
    if (! validDeck (leader) || ! validDeck (follower) || follower == leader)
        return false;
    const auto f = static_cast<size_t> (follower), l = static_cast<size_t> (leader);
    if (f >= count || l >= count || ! slots[f].enabled)
        return false;
    const auto& lg = slots[l].grid;
    const auto& fg = slots[f].grid;
    return lg != nullptr && fg != nullptr && lg->getBpm() > 0.0 && fg->getBpm() > 0.0 && decks[l].playing;
}

double Sync::phaseOf (const Slot& s, const SyncDeckState& d) const noexcept
{
    const double unit = mode == SyncMode::Bar ? std::max (1, s.grid->getBeatsPerBar()) : 1.0;
    const double beat = s.grid->beatAtFrame (d.position) / unit;
    return beat - std::floor (beat);
}

double Sync::errorBeats (const std::vector<SyncDeckState>& decks, int follower) const noexcept
{
    const auto& f = slots[static_cast<size_t> (follower)];
    const auto& l = slots[static_cast<size_t> (leader)];
    const double unit = mode == SyncMode::Bar ? std::max (1, f.grid->getBeatsPerBar()) : 1.0;
    return wrapHalf (phaseOf (l, decks[static_cast<size_t> (leader)]) - phaseOf (f, decks[static_cast<size_t> (follower)])) * unit;
}

double Sync::phaseError (const std::vector<SyncDeckState>& decks, int follower) const
{
    const std::scoped_lock lock (mutex);
    return canSync (decks, follower) ? errorBeats (decks, follower) : 0.0;
}

std::optional<double> Sync::followerRate (const std::vector<SyncDeckState>& decks, int follower) const
{
    const auto& l = slots[static_cast<size_t> (leader)];
    const auto& f = slots[static_cast<size_t> (follower)];
    const auto& ld = decks[static_cast<size_t> (leader)];
    const auto& fd = decks[static_cast<size_t> (follower)];

    const double base = ld.rate * l.grid->getBpm() / f.grid->getBpm();
    if (! fd.playing)
        return base;   // phase is meaningless until it plays

    const double error = errorBeats (decks, follower);
    const double nudge = std::abs (error) < kDeadband ? 0.0 : std::clamp (kPhaseGain * error, -kMaxNudge, kMaxNudge);
    return base * (1.0 + nudge);
}

std::vector<std::optional<double>> Sync::step (const std::vector<SyncDeckState>& decks) const
{
    const std::scoped_lock lock (mutex);
    std::vector<std::optional<double>> out (decks.size());
    for (int d = 0; d < static_cast<int> (decks.size()); ++d)
        if (canSync (decks, d))
            out[static_cast<size_t> (d)] = followerRate (decks, d);
    return out;
}

void Sync::apply (const std::vector<Deck*>& decks) const
{
    std::vector<SyncDeckState> states;
    for (const auto* d : decks)
        states.push_back (d == nullptr ? SyncDeckState {} : SyncDeckState { d->getPositionFrames(), d->getRate(), d->isPlaying() });

    const auto rates = step (states);
    for (size_t i = 0; i < rates.size() && i < decks.size(); ++i)
        if (rates[i].has_value() && decks[i] != nullptr)
            decks[i]->setRate (*rates[i]);
}

} // namespace openmixer
