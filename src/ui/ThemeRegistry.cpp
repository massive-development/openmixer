#include "ThemeRegistry.h"

#include "Theme.h"
#include "ThemeBinaryData.h"

#include <algorithm>
#include <map>

namespace openmixer::ui
{

juce::File ThemeRegistry::defaultUserDirectory()
{
    return juce::File::getSpecialLocation (juce::File::userApplicationDataDirectory).getChildFile ("OpenMixer").getChildFile ("themes");
}

juce::Result ThemeRegistry::addJson (const juce::String& json, Source source, const juce::String& owner)
{
    JUCE_ASSERT_MESSAGE_THREAD
    auto parsed = parseTheme (json);
    if (parsed.status.failed())
        return parsed.status;
    addEntry ({ std::move (parsed.info), std::move (parsed.tokens), source, owner });
    entriesChanged();
    return juce::Result::ok();
}

void ThemeRegistry::add (ThemeInfo info, ThemeTokens tokens, Source source, const juce::String& owner)
{
    JUCE_ASSERT_MESSAGE_THREAD
    addEntry ({ std::move (info), std::move (tokens), source, owner });
    entriesChanged();
}

// Same id from the same provider replaces; different providers stack and precedence picks.
void ThemeRegistry::addEntry (Entry&& entry)
{
    entries.erase (std::remove_if (entries.begin(), entries.end(), [&entry] (const Entry& e)
                                   { return e.info.id == entry.info.id && e.source == entry.source && e.owner == entry.owner; }),
                   entries.end());
    entries.push_back (std::move (entry));
}

int ThemeRegistry::loadBuiltIns()
{
    JUCE_ASSERT_MESSAGE_THREAD
    int loaded = 0;
    for (int i = 0; i < openmixer_themes::namedResourceListSize; ++i)
    {
        int size = 0;
        const auto* data = openmixer_themes::getNamedResource (openmixer_themes::namedResourceList[i], size);
        const auto json = juce::String::createStringFromData (data, size);
        if (addJson (json, Source::BuiltIn, openmixer_themes::namedResourceList[i]).wasOk())
            ++loaded;
    }
    return loaded;
}

int ThemeRegistry::loadUserDirectory (const juce::File& dir, juce::StringArray* errors)
{
    JUCE_ASSERT_MESSAGE_THREAD
    entries.erase (std::remove_if (entries.begin(), entries.end(), [] (const Entry& e) { return e.source == Source::User; }), entries.end());
    int loaded = 0;
    auto files = dir.findChildFiles (juce::File::findFiles, false, "*.json");
    std::sort (files.begin(), files.end(), [] (const juce::File& a, const juce::File& b) { return a.getFileName() < b.getFileName(); });
    for (const auto& file : files)
    {
        auto parsed = parseThemeFile (file);   // a bad file is skipped, never fatal
        if (parsed.status.failed())
        {
            if (errors != nullptr)
                errors->add (file.getFileName() + ": " + parsed.status.getErrorMessage());
            continue;
        }
        addEntry ({ std::move (parsed.info), std::move (parsed.tokens), Source::User, file.getFileName() });
        ++loaded;
    }
    entriesChanged();
    return loaded;
}

void ThemeRegistry::removeOwner (const juce::String& owner)
{
    JUCE_ASSERT_MESSAGE_THREAD
    entries.erase (std::remove_if (entries.begin(), entries.end(), [&owner] (const Entry& e) { return e.owner == owner; }), entries.end());
    entriesChanged();
}

void ThemeRegistry::removeSource (Source source)
{
    JUCE_ASSERT_MESSAGE_THREAD
    entries.erase (std::remove_if (entries.begin(), entries.end(), [source] (const Entry& e) { return e.source == source; }), entries.end());
    entriesChanged();
}

// Highest source wins; among equals the later add (higher index) wins.
const ThemeRegistry::Entry* ThemeRegistry::find (const juce::String& id) const
{
    const Entry* best = nullptr;
    for (const auto& e : entries)
        if (e.info.id == id && (best == nullptr || e.source >= best->source))
            best = &e;
    return best;
}

std::vector<ThemeRegistry::Entry> ThemeRegistry::getThemes() const
{
    std::map<juce::String, const Entry*> byId;
    for (const auto& e : entries)
        if (const auto* winner = find (e.info.id))
            byId[e.info.id] = winner;
    std::vector<Entry> out;
    for (const auto& [id, entry] : byId)
        out.push_back (*entry);
    return out;
}

bool ThemeRegistry::select (const juce::String& id)
{
    JUCE_ASSERT_MESSAGE_THREAD
    const auto* entry = find (id);
    if (entry == nullptr)
        return false;
    activeId = id;
    theme::setCurrent (entry->tokens);
    sendChangeMessage();
    return true;
}

// The active theme may have been overridden or removed: re-apply it, else fall back to the default.
void ThemeRegistry::entriesChanged()
{
    if (activeId.isNotEmpty())
    {
        const auto* entry = find (activeId);
        if (entry == nullptr)
        {
            activeId = find (defaultId()) != nullptr ? juce::String (defaultId()) : juce::String();
            entry = find (activeId);
        }
        theme::setCurrent (entry != nullptr ? entry->tokens : ThemeTokens::builtinDark());
    }
    sendChangeMessage();
}

} // namespace openmixer::ui
