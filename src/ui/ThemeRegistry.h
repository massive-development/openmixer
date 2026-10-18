// Theme registry (Todo 25a): built-in, plugin-provided and user themes, with live switching.
// Precedence for one id: User > Plugin > BuiltIn; within a source the latest add wins.
// Selecting a theme makes it the current theme (theme::setCurrent), which repaints every window
// through the Glass look-and-feel. Message thread only. Use one registry per app.
//
// A plugin ships a theme with PluginHost::registerTheme (json); the app forwards what
// PluginRegistry::getThemes() reports with addJson (Source::Plugin, pluginId) and drops it again
// with removeOwner (pluginId) when the plugin unloads. Code can also call add() directly.
#pragma once

#include "ThemeJson.h"

#include <juce_events/juce_events.h>

#include <vector>

namespace openmixer::ui
{

class ThemeRegistry : public juce::ChangeBroadcaster   // change message: the list or the active theme changed
{
public:
    enum class Source { BuiltIn = 0, Plugin = 1, User = 2 };   // higher wins

    struct Entry
    {
        ThemeInfo info;
        ThemeTokens tokens;
        Source source = Source::BuiltIn;
        juce::String owner;   // plugin id or file name; lets one provider be removed as a unit
    };

    static const char* defaultId() { return "openmixer.dark"; }
    static juce::File defaultUserDirectory();   // <userApplicationData>/OpenMixer/themes

    juce::Result addJson (const juce::String& json, Source source, const juce::String& owner = {});
    void add (ThemeInfo info, ThemeTokens tokens, Source source, const juce::String& owner = {});

    int loadBuiltIns();   // embedded JSON resources; returns how many loaded
    int loadUserDirectory (const juce::File& dir, juce::StringArray* errors = nullptr);   // replaces earlier user themes

    void removeOwner (const juce::String& owner);
    void removeSource (Source source);

    std::vector<Entry> getThemes() const;   // one per id after precedence, in id order
    const Entry* find (const juce::String& id) const;   // pointer valid until the next change

    bool select (const juce::String& id);   // false if unknown; the current theme is unchanged
    juce::String getActiveId() const { return activeId; }

private:
    void addEntry (Entry&& entry);
    void entriesChanged();

    std::vector<Entry> entries;
    juce::String activeId;
};

} // namespace openmixer::ui
