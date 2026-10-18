// Persisted list of watched folders (absolute paths). Same threading rules as TrackDao.
#pragma once

#include "Database.h"

#include <vector>

namespace openmixer::library
{

class WatchFolderDao
{
public:
    explicit WatchFolderDao (Database& database) noexcept : db (database) {}

    // Adding an existing path is a no-op success.
    juce::Result add (const juce::String& path);
    juce::Result remove (const juce::String& path);
    std::vector<juce::String> list (juce::Result* status = nullptr);

private:
    Database& db;
};

} // namespace openmixer::library
