#include "WatchFolderDao.h"

namespace openmixer::library
{

juce::Result WatchFolderDao::add (const juce::String& path)
{
    auto s = db.prepare ("INSERT OR IGNORE INTO watch_folders (path) VALUES (?1);");
    s.bindAll (path);
    return s.run();
}

juce::Result WatchFolderDao::remove (const juce::String& path)
{
    auto s = db.prepare ("DELETE FROM watch_folders WHERE path=?1;");
    s.bindAll (path);
    return s.run();
}

std::vector<juce::String> WatchFolderDao::list (juce::Result* status)
{
    auto s = db.prepare ("SELECT path FROM watch_folders ORDER BY id;");
    std::vector<juce::String> paths;
    while (s.nextRow())
        paths.push_back (s.text (0));
    db.report (s, status);
    return paths;
}

} // namespace openmixer::library
