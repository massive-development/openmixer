#include "Library.h"

namespace openmixer::library
{

Library::Library (OpenKey, std::unique_ptr<Database> database) noexcept
    : db (std::move (database)), trackDao (*db), cueDao (*db), playlistDao (*db)
{
}

std::unique_ptr<Library> Library::open (const juce::String& path, Database::OpenMode mode,
                                        const std::vector<Migration>& migrations, juce::String* error)
{
    auto db = Database::open (path, mode, error);
    if (db == nullptr)
        return nullptr;

    // A read-only connection cannot migrate; the writer already did.
    auto result = mode == Database::OpenMode::ReadWrite ? migrate (*db, migrations) : juce::Result::ok();
    if (result.failed())
    {
        if (error != nullptr)
            *error = result.getErrorMessage();
        return nullptr;
    }
    return std::make_unique<Library> (OpenKey {}, std::move (db));
}

} // namespace openmixer::library
