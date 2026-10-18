// Persisted track prep queue: which tracks are waiting to be analysed, in what order, and how each
// ended. Same threading rules as TrackDao (use from the thread that owns the Database).
// Processing order is priority (highest first), then position; position is the user's list order.
#pragma once

#include "Database.h"

#include <vector>

namespace openmixer::library
{

enum class PrepStatus { Queued, Running, Done, Failed, Cancelled };

struct PrepEntry
{
    std::int64_t trackId = 0;
    int position = 0;                  // user order, dense 0..n-1 after move()
    int priority = 0;                  // > 0 jumps ahead of normal entries
    PrepStatus status = PrepStatus::Queued;
    float progress = 0.0f;             // 0..1; live value only in PrepQueue::list(), 1 once Done
    juce::String error;
};

const char* toString (PrepStatus status) noexcept;

class PrepQueueDao
{
public:
    explicit PrepQueueDao (Database& database) noexcept : db (database) {}

    // Appends at the end; an existing finished entry is re-queued. A running entry is left alone.
    // Fails if the track does not exist.
    juce::Result enqueue (std::int64_t trackId);
    // Runs next among queued entries (the latest call goes first).
    juce::Result prioritise (std::int64_t trackId);
    // Moves within the user order; clamps to the list.
    juce::Result move (std::int64_t trackId, int index);
    juce::Result remove (std::int64_t trackId);
    juce::Result clearFinished();   // drops Done, Failed and Cancelled rows
    juce::Result setStatus (std::int64_t trackId, PrepStatus status, const juce::String& error = {});
    juce::Result resetRunning();    // crash recovery: Running -> Queued

    std::optional<PrepEntry> find (std::int64_t trackId, juce::Result* status = nullptr);
    std::vector<PrepEntry> all (juce::Result* status = nullptr);   // user order
    std::optional<PrepEntry> nextQueued (juce::Result* status = nullptr);

private:
    std::vector<std::int64_t> orderedIds (juce::Result* status);

    Database& db;
};

} // namespace openmixer::library
