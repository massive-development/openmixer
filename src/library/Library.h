// Composition root for the library database: opens, migrates and hands out the DAOs.
// Threading: owned and written by one thread; open a second Library with OpenMode::ReadOnly for the UI.
#pragma once

#include "CueDao.h"
#include "Migrations.h"
#include "PlaylistDao.h"
#include "TrackDao.h"
#include "TrackFileDao.h"
#include "WatchFolderDao.h"

namespace openmixer::library
{

class Library
{
public:
    // Later subsystems pass extra migrations after coreMigrations(); read-only opens skip migrating.
    static std::unique_ptr<Library> open (const juce::String& path, Database::OpenMode mode = Database::OpenMode::ReadWrite,
                                          const std::vector<Migration>& migrations = coreMigrations(),
                                          juce::String* error = nullptr);

    Database& database() noexcept { return *db; }
    TrackDao& tracks() noexcept { return trackDao; }
    CueDao& cues() noexcept { return cueDao; }
    PlaylistDao& playlists() noexcept { return playlistDao; }
    TrackFileDao& trackFiles() noexcept { return fileDao; }
    WatchFolderDao& watchFolders() noexcept { return folderDao; }

private:
    struct OpenKey { explicit OpenKey() = default; };

public:
    Library (OpenKey, std::unique_ptr<Database> database) noexcept;

private:
    std::unique_ptr<Database> db;
    TrackDao trackDao;
    CueDao cueDao;
    PlaylistDao playlistDao;
    TrackFileDao fileDao;
    WatchFolderDao folderDao;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR (Library)
};

} // namespace openmixer::library
