// Hotcues and saved loops per track. Positions are engine frames (same unit as HotcueBank).
// Non-owning view of a Database; use from the thread that owns it (see Database.h).
#pragma once

#include "Database.h"

#include <vector>

namespace openmixer::library
{

struct CuePoint
{
    std::int64_t trackId = 0;
    int slot = 0;                       // hotcue index, 0-based
    std::int64_t positionFrames = 0;
    std::uint32_t colourArgb = 0;
    juce::String label;
};

struct SavedLoop
{
    std::int64_t trackId = 0;
    int slot = 0;
    std::int64_t startFrames = 0;
    std::int64_t endFrames = 0;         // must be > startFrames
    std::uint32_t colourArgb = 0;
    juce::String label;
};

class CueDao
{
public:
    explicit CueDao (Database& database) noexcept : db (database) {}

    // Insert or replace the cue/loop in (trackId, slot). Fails for an unknown track.
    juce::Result setCue (const CuePoint& cue);
    juce::Result clearCue (std::int64_t trackId, int slot);
    std::vector<CuePoint> cues (std::int64_t trackId, juce::Result* status = nullptr);   // ordered by slot

    juce::Result setLoop (const SavedLoop& loop);
    juce::Result clearLoop (std::int64_t trackId, int slot);
    std::vector<SavedLoop> loops (std::int64_t trackId, juce::Result* status = nullptr);

private:
    Database& db;
};

} // namespace openmixer::library
