#include "ThemeJson.h"

#include <array>

namespace openmixer::ui
{

namespace
{
    constexpr int kMaxAccents = 8, kMaxSpacing = 64;
    constexpr float kMinType = 6.0F, kMaxType = 64.0F, kMaxRadius = 48.0F;

    struct ColourKey { const char* key; juce::Colour ThemeTokens::*field; };
    struct IntKey { const char* key; int ThemeTokens::*field; };
    struct FloatKey { const char* key; float ThemeTokens::*field; float lo, hi; };

    const std::array<ColourKey, 25> kColours { {
        { "backdropTop", &ThemeTokens::backdropTop },     { "backdropBottom", &ThemeTokens::backdropBottom },
        { "glowCool", &ThemeTokens::glowCool },           { "glowWarm", &ThemeTokens::glowWarm },
        { "surface", &ThemeTokens::surface },             { "surfaceRaised", &ThemeTokens::surfaceRaised },
        { "surfaceHover", &ThemeTokens::surfaceHover },   { "well", &ThemeTokens::well },
        { "border", &ThemeTokens::border },               { "highlight", &ThemeTokens::highlight },
        { "shadow", &ThemeTokens::shadow },               { "textPrimary", &ThemeTokens::textPrimary },
        { "textSecondary", &ThemeTokens::textSecondary }, { "textMuted", &ThemeTokens::textMuted },
        { "onAccent", &ThemeTokens::onAccent },           { "meterLow", &ThemeTokens::meterLow },
        { "meterMid", &ThemeTokens::meterMid },           { "meterHigh", &ThemeTokens::meterHigh },
        { "master", &ThemeTokens::master },               { "popup", &ThemeTokens::popup },
        { "capTop", &ThemeTokens::capTop },               { "capBottom", &ThemeTokens::capBottom },
        { "waveLow", &ThemeTokens::waveLow },             { "waveMid", &ThemeTokens::waveMid },
        { "waveHigh", &ThemeTokens::waveHigh } } };

    const std::array<IntKey, 3> kSpacing { { { "gap", &ThemeTokens::gap }, { "pad", &ThemeTokens::pad },
                                             { "innerGap", &ThemeTokens::innerGap } } };
    const std::array<FloatKey, 2> kRadii { { { "card", &ThemeTokens::cardRadius, 0.0F, kMaxRadius },
                                             { "control", &ThemeTokens::controlRadius, 0.0F, kMaxRadius } } };
    const std::array<FloatKey, 7> kType { { { "small", &ThemeTokens::typeSmall, kMinType, kMaxType },
                                            { "caption", &ThemeTokens::typeCaption, kMinType, kMaxType },
                                            { "body", &ThemeTokens::typeBody, kMinType, kMaxType },
                                            { "label", &ThemeTokens::typeLabel, kMinType, kMaxType },
                                            { "title", &ThemeTokens::typeTitle, kMinType, kMaxType },
                                            { "heading", &ThemeTokens::typeHeading, kMinType, kMaxType },
                                            { "mono", &ThemeTokens::typeMono, kMinType, kMaxType } } };
    constexpr float kMaxPx = 400.0F;
    const std::array<FloatKey, 14> kDeck { { { "headerHeight", &ThemeTokens::deckHeaderHeight, 0.0F, kMaxPx },
                                             { "overviewHeight", &ThemeTokens::deckOverviewHeight, 0.0F, kMaxPx },
                                             { "transportHeight", &ThemeTokens::deckTransportHeight, 0.0F, kMaxPx },
                                             { "rowHeight", &ThemeTokens::deckRowHeight, 0.0F, kMaxPx },
                                             { "pitchWidth", &ThemeTokens::deckPitchWidth, 0.0F, kMaxPx },
                                             { "minZoomHeight", &ThemeTokens::deckMinZoomHeight, 0.0F, kMaxPx },
                                             { "readoutWidth", &ThemeTokens::deckReadoutWidth, 0.0F, kMaxPx },
                                             { "rangeBoxHeight", &ThemeTokens::deckRangeBoxHeight, 0.0F, kMaxPx },
                                             { "playheadWidth", &ThemeTokens::playheadWidth, 0.0F, kMaxRadius },
                                             { "cueMarkerWidth", &ThemeTokens::cueMarkerWidth, 0.0F, kMaxRadius },
                                             { "gridBarAlpha", &ThemeTokens::gridBarAlpha, 0.0F, 1.0F },
                                             { "gridBeatAlpha", &ThemeTokens::gridBeatAlpha, 0.0F, 1.0F },
                                             { "loopActiveAlpha", &ThemeTokens::loopActiveAlpha, 0.0F, 1.0F },
                                             { "loopIdleAlpha", &ThemeTokens::loopIdleAlpha, 0.0F, 1.0F } } };
    const std::array<FloatKey, 16> kMixer { { { "knobHeight", &ThemeTokens::mixerKnobHeight, 0.0F, kMaxPx },
                                              { "buttonHeight", &ThemeTokens::mixerButtonHeight, 0.0F, kMaxPx },
                                              { "meterWidth", &ThemeTokens::mixerMeterWidth, 0.0F, kMaxPx },
                                              { "masterWidth", &ThemeTokens::mixerMasterWidth, 0.0F, kMaxPx },
                                              { "tabHeight", &ThemeTokens::mixerTabHeight, 0.0F, kMaxPx },
                                              { "crossfaderHeight", &ThemeTokens::mixerCrossfaderHeight, 0.0F, kMaxPx },
                                              { "cueHeight", &ThemeTokens::mixerCueHeight, 0.0F, kMaxPx },
                                              { "meterFloorDb", &ThemeTokens::meterFloorDb, -96.0F, -6.0F },
                                              { "meterMidDb", &ThemeTokens::meterMidDb, -96.0F, 0.0F },
                                              { "meterHighDb", &ThemeTokens::meterHighDb, -96.0F, 0.0F },
                                              { "meterClipLevel", &ThemeTokens::meterClipLevel, 0.1F, 1.0F },
                                              { "meterReleaseSeconds", &ThemeTokens::meterReleaseSeconds, 0.01F, 10.0F },
                                              { "meterHoldSeconds", &ThemeTokens::meterHoldSeconds, 0.0F, 10.0F },
                                              { "meterClipSeconds", &ThemeTokens::meterClipSeconds, 0.0F, 30.0F },
                                              { "meterSegment", &ThemeTokens::meterSegment, 1.0F, 20.0F },
                                              { "meterSegmentGap", &ThemeTokens::meterSegmentGap, 0.0F, 10.0F } } };
    const std::array<FloatKey, 2> kTranslucency { { { "surface", &ThemeTokens::surfaceOpacity, 0.0F, 1.0F },
                                                    { "backdrop", &ThemeTokens::backdropOpacity, 0.0F, 1.0F } } };

    juce::Result fail (const juce::String& where, const juce::String& why) { return juce::Result::fail (where + ": " + why); }

    bool isThemeId (const juce::String& id)
    {
        return id.isNotEmpty() && id.containsOnly ("abcdefghijklmnopqrstuvwxyz0123456789._-");
    }

    // Reports keys the schema does not know (typos) without rejecting the theme.
    void warnUnknown (const juce::DynamicObject& obj, const juce::StringArray& known, const juce::String& where, juce::StringArray& warnings)
    {
        for (const auto& prop : obj.getProperties())
            if (! known.contains (prop.name.toString()))
                warnings.add ("unknown key '" + where + prop.name.toString() + "' ignored");
    }

    const juce::DynamicObject* sectionOf (const juce::DynamicObject& root, const char* name, juce::Result& status)
    {
        if (! root.hasProperty (name))
            return nullptr;
        const auto* obj = root.getProperty (name).getDynamicObject();
        if (obj == nullptr)
            status = fail (name, "must be an object");
        return obj;
    }

    juce::Result readColours (const juce::DynamicObject& section, ThemeTokens& t, juce::StringArray& warnings)
    {
        juce::StringArray known;
        for (const auto& k : kColours)
        {
            known.add (k.key);
            if (! section.hasProperty (k.key))
                continue;
            if (! parseColourHex (section.getProperty (k.key).toString(), t.*(k.field)))
                return fail (juce::String ("colours.") + k.key, "expected \"#RRGGBB\" or \"#AARRGGBB\"");
        }
        warnUnknown (section, known, "colours.", warnings);
        return juce::Result::ok();
    }

    juce::Result readAccents (const juce::var& value, ThemeTokens& t)
    {
        const auto* arr = value.getArray();
        if (arr == nullptr || arr->isEmpty() || arr->size() > kMaxAccents)
            return fail ("accents", "expected 1-" + juce::String (kMaxAccents) + " colours");
        std::vector<juce::Colour> accents;
        for (const auto& item : *arr)
        {
            juce::Colour c;
            if (! parseColourHex (item.toString(), c))
                return fail ("accents", "expected \"#RRGGBB\" or \"#AARRGGBB\"");
            accents.push_back (c);
        }
        t.accents = std::move (accents);
        return juce::Result::ok();
    }

    juce::Result readSpacing (const juce::DynamicObject& section, ThemeTokens& t, juce::StringArray& warnings)
    {
        juce::StringArray known;
        for (const auto& k : kSpacing)
        {
            known.add (k.key);
            if (! section.hasProperty (k.key))
                continue;
            const auto v = section.getProperty (k.key);
            if (! (v.isInt() || v.isInt64()) || static_cast<int> (v) < 0 || static_cast<int> (v) > kMaxSpacing)
                return fail (juce::String ("spacing.") + k.key, "expected an integer 0-" + juce::String (kMaxSpacing));
            t.*(k.field) = static_cast<int> (v);
        }
        warnUnknown (section, known, "spacing.", warnings);
        return juce::Result::ok();
    }

    template <typename Keys>
    juce::Result readFloats (const juce::DynamicObject& section, const Keys& keys, const juce::String& where, ThemeTokens& t,
                             juce::StringArray& warnings)
    {
        juce::StringArray known;
        for (const auto& k : keys)
        {
            known.add (k.key);
            if (! section.hasProperty (k.key))
                continue;
            const auto v = section.getProperty (k.key);
            const auto f = static_cast<float> (static_cast<double> (v));
            if (! (v.isInt() || v.isInt64() || v.isDouble()) || ! (f >= k.lo && f <= k.hi))
                return fail (where + "." + k.key, "expected a number " + juce::String (k.lo) + "-" + juce::String (k.hi));
            t.*(k.field) = f;
        }
        warnUnknown (section, known, where + ".", warnings);
        return juce::Result::ok();
    }

    juce::Result readIdentity (const juce::DynamicObject& root, ThemeInfo& info)
    {
        info.id = root.getProperty ("id").toString();
        if (! isThemeId (info.id))
            return fail ("id", "required, lowercase letters, digits, '.', '_' or '-'");
        info.name = root.hasProperty ("name") ? root.getProperty ("name").toString().trim() : info.id;
        info.author = root.getProperty ("author").toString();
        return info.name.isEmpty() ? fail ("name", "must not be empty") : juce::Result::ok();
    }

    // Runs `read` for one optional object section; stops at the first failure.
    template <typename Read>
    void withSection (const juce::DynamicObject& root, const char* name, juce::Result& status, Read&& read)
    {
        if (status.failed())
            return;
        const auto* section = sectionOf (root, name, status);
        if (section != nullptr && status.wasOk())
            status = read (*section);
    }

    juce::Result fill (const juce::DynamicObject& root, ParsedTheme& out)
    {
        auto status = readIdentity (root, out.info);
        auto& t = out.tokens;
        auto& w = out.warnings;
        withSection (root, "colours", status, [&] (const auto& s) { return readColours (s, t, w); });
        withSection (root, "spacing", status, [&] (const auto& s) { return readSpacing (s, t, w); });
        withSection (root, "radii", status, [&] (const auto& s) { return readFloats (s, kRadii, "radii", t, w); });
        withSection (root, "type", status, [&] (const auto& s) { return readFloats (s, kType, "type", t, w); });
        withSection (root, "translucency", status, [&] (const auto& s) { return readFloats (s, kTranslucency, "translucency", t, w); });
        withSection (root, "deck", status, [&] (const auto& s) { return readFloats (s, kDeck, "deck", t, w); });
        withSection (root, "mixer", status, [&] (const auto& s) { return readFloats (s, kMixer, "mixer", t, w); });
        if (status.wasOk() && root.hasProperty ("accents"))
            status = readAccents (root.getProperty ("accents"), t);
        warnUnknown (root, { "id", "name", "author", "colours", "accents", "spacing", "radii", "type", "translucency", "deck", "mixer" }, "", w);
        return status;
    }
}

bool parseColourHex (const juce::String& text, juce::Colour& out)
{
    if (! text.startsWithChar ('#'))
        return false;
    const auto digits = text.substring (1);
    if ((digits.length() != 6 && digits.length() != 8) || ! digits.containsOnly ("0123456789abcdefABCDEF"))
        return false;
    const auto value = static_cast<juce::uint32> (digits.getHexValue64());
    out = juce::Colour (digits.length() == 6 ? (0xFF000000U | value) : value);
    return true;
}

ParsedTheme parseTheme (const juce::String& json)
{
    ParsedTheme out;
    juce::var root;
    if (const auto parsed = juce::JSON::parse (json, root); parsed.failed())
    {
        out.status = juce::Result::fail ("invalid JSON: " + parsed.getErrorMessage());
        return out;
    }
    const auto* obj = root.getDynamicObject();
    out.status = obj != nullptr ? fill (*obj, out) : juce::Result::fail ("theme must be a JSON object");
    if (out.status.failed())
    {
        out.tokens = ThemeTokens::builtinDark();   // never hand back a half-filled token set
        out.warnings.clear();
    }
    return out;
}

ParsedTheme parseThemeFile (const juce::File& file)
{
    if (! file.existsAsFile())
    {
        ParsedTheme out;
        out.status = juce::Result::fail ("not a file: " + file.getFullPathName());
        return out;
    }
    return parseTheme (file.loadFileAsString());
}

} // namespace openmixer::ui
