// Plain data for one library track row. Value type; safe to copy between threads.
#pragma once

#include <juce_core/juce_core.h>

#include <cstdint>
#include <optional>

namespace openmixer::library
{

struct TrackRecord
{
    std::int64_t id = 0;                          // 0 until inserted
    juce::String path;                            // unique; absolute file path
    juce::String contentHash;                     // SHA-256 of file bytes, matches the analysis cache key
    juce::String title, artist, album, genre;
    int year = 0;
    double durationSec = 0.0;
    std::optional<double> bpm;                    // unset until analysed
    juce::String key;                             // musical key, e.g. "Am"
    juce::String camelot;                         // e.g. "8A"
    int rating = 0;                               // 0..5
    int playCount = 0;
    std::int64_t dateAddedMs = 0;                 // 0 on insert means "now"
    std::optional<std::int64_t> lastPlayedMs;
    juce::String comment;
};

} // namespace openmixer::library
