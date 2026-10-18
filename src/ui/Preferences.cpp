#include "Preferences.h"

namespace openmixer::ui
{

namespace
{
    constexpr int kMaxPair = 31;
    constexpr int kMaxIntervalMinutes = 24 * 60;

    juce::File dataDir() { return juce::File::getSpecialLocation (juce::File::userApplicationDataDirectory).getChildFile ("OpenMixer"); }

    juce::String stringOr (const juce::var& v, const juce::String& fallback) { return v.isString() ? v.toString() : fallback; }
    bool boolOr (const juce::var& v, bool fallback) { return v.isBool() ? static_cast<bool> (v) : fallback; }
    int intOr (const juce::var& v, int lo, int hi, int fallback)
    {
        return (v.isInt() || v.isInt64()) && static_cast<int> (v) >= lo && static_cast<int> (v) <= hi ? static_cast<int> (v) : fallback;
    }

    juce::var section (const juce::var& root, const char* name) { return root.getProperty (name, {}); }

    void readAudio (const juce::var& s, AudioPrefs& a)
    {
        a.deviceState = stringOr (s.getProperty ("deviceState", {}), a.deviceState);
        a.masterPair = intOr (s.getProperty ("masterPair", {}), 0, kMaxPair, a.masterPair);
        a.cuePair = intOr (s.getProperty ("cuePair", {}), -1, kMaxPair, a.cuePair);
    }

    void readMidi (const juce::var& s, MidiPrefs& m)
    {
        m.enableAll = boolOr (s.getProperty ("enableAll", {}), m.enableAll);
        m.mapFile = stringOr (s.getProperty ("mapFile", {}), m.mapFile);
        if (const auto* list = s.getProperty ("enabledInputs", {}).getArray())
        {
            m.enabledInputs.clear();
            for (const auto& item : *list)
                if (item.isString())
                    m.enabledInputs.addIfNotAlreadyThere (item.toString());
        }
    }

    void readLibrary (const juce::var& s, LibraryPrefs& l)
    {
        l.scanOnStart = boolOr (s.getProperty ("scanOnStart", {}), l.scanOnStart);
        l.scanIntervalMinutes = intOr (s.getProperty ("scanIntervalMinutes", {}), 1, kMaxIntervalMinutes, l.scanIntervalMinutes);
    }
}

juce::File Preferences::defaultFile()
{
    const juce::File custom (juce::SystemStats::getEnvironmentVariable ("OPENMIXER_PREFS", {}));
    return custom != juce::File() ? custom : dataDir().getChildFile ("preferences.json");
}

juce::File Preferences::defaultPluginFolder() { return dataDir().getChildFile ("plugins"); }
juce::File Preferences::defaultMapFile() { return dataDir().getChildFile ("controller_map.json"); }

juce::File Preferences::effectivePluginFolder() const
{
    return pluginFolder.isEmpty() ? defaultPluginFolder() : juce::File (pluginFolder);
}

juce::File Preferences::effectiveMapFile() const
{
    return midi.mapFile.isEmpty() ? defaultMapFile() : juce::File (midi.mapFile);
}

juce::String Preferences::toJson() const
{
    auto* root = new juce::DynamicObject();
    juce::var rootVar (root);
    root->setProperty ("version", kVersion);
    root->setProperty ("theme", theme);

    auto* a = new juce::DynamicObject();
    a->setProperty ("deviceState", audio.deviceState);
    a->setProperty ("masterPair", audio.masterPair);
    a->setProperty ("cuePair", audio.cuePair);
    root->setProperty ("audio", juce::var (a));

    auto* m = new juce::DynamicObject();
    juce::Array<juce::var> inputs;
    for (const auto& i : midi.enabledInputs)
        inputs.add (i);
    m->setProperty ("enableAll", midi.enableAll);
    m->setProperty ("enabledInputs", inputs);
    m->setProperty ("mapFile", midi.mapFile);
    root->setProperty ("midi", juce::var (m));

    auto* l = new juce::DynamicObject();
    l->setProperty ("scanOnStart", library.scanOnStart);
    l->setProperty ("scanIntervalMinutes", library.scanIntervalMinutes);
    root->setProperty ("library", juce::var (l));

    auto* p = new juce::DynamicObject();
    p->setProperty ("folder", pluginFolder);
    root->setProperty ("plugins", juce::var (p));
    return juce::JSON::toString (rootVar, false);
}

juce::Result Preferences::fromJson (const juce::String& json, Preferences& out)
{
    juce::var root;
    if (const auto parsed = juce::JSON::parse (json, root); parsed.failed() || ! root.isObject())
        return juce::Result::fail ("preferences: not a JSON object");
    const int version = intOr (root.getProperty ("version", {}), 0, 1'000'000, 0);
    if (version < 1 || version > kVersion)
        return juce::Result::fail ("preferences: unsupported version " + juce::String (version));

    Preferences p;   // start from defaults so a bad field only loses itself
    p.theme = stringOr (root.getProperty ("theme", {}), p.theme);
    readAudio (section (root, "audio"), p.audio);
    readMidi (section (root, "midi"), p.midi);
    readLibrary (section (root, "library"), p.library);
    p.pluginFolder = stringOr (section (root, "plugins").getProperty ("folder", {}), p.pluginFolder);
    out = std::move (p);
    return juce::Result::ok();
}

juce::Result Preferences::load (const juce::File& file, Preferences& out)
{
    out = Preferences();
    if (! file.existsAsFile())
        return juce::Result::ok();
    const auto result = fromJson (file.loadFileAsString(), out);
    if (result.failed())
    {
        out = Preferences();
        file.copyFileTo (file.withFileExtension (file.getFileExtension() + ".bad"));   // keep the evidence; the next save replaces it
    }
    return result;
}

juce::Result Preferences::save (const juce::File& file) const
{
    if (const auto dir = file.getParentDirectory(); ! dir.createDirectory())
        return juce::Result::fail ("cannot create " + dir.getFullPathName());
    return file.replaceWithText (toJson()) ? juce::Result::ok() : juce::Result::fail ("cannot write " + file.getFullPathName());
}

} // namespace openmixer::ui
