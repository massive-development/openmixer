// OpenMixer ThemeJsonTest - theme file parsing, validation and fallback to the built-in default
#include <JuceHeader.h>
#include "ui/Theme.h"
#include "ui/ThemeJson.h"

#include <gtest/gtest.h>

using namespace openmixer::ui;

namespace
{
const char* kMinimal = R"({ "id": "t.min" })";

ParsedTheme parse (const juce::String& json) { return parseTheme (json); }
}

TEST (ThemeJsonTest, MinimalThemeInheritsEverythingFromDark)
{
    const auto p = parse (kMinimal);
    ASSERT_TRUE (p.status.wasOk()) << p.status.getErrorMessage();
    EXPECT_EQ (p.info.id, "t.min");
    EXPECT_EQ (p.info.name, "t.min");   // name defaults to the id
    const auto& dark = ThemeTokens::builtinDark();
    EXPECT_EQ (p.tokens.backdropTop, dark.backdropTop);
    EXPECT_EQ (p.tokens.gap, dark.gap);
    EXPECT_EQ (p.tokens.accents.size(), dark.accents.size());
}

TEST (ThemeJsonTest, ParsesEverySection)
{
    const auto p = parse (R"({
        "id": "t.full", "name": "Full", "author": "me",
        "colours": { "textPrimary": "#112233", "well": "#80112233" },
        "accents": ["#FF0000", "#00FF00"],
        "spacing": { "gap": 6, "pad": 7, "innerGap": 3 },
        "radii": { "card": 20, "control": 4.5 },
        "type": { "title": 22, "mono": 13.5 },
        "translucency": { "surface": 0.5, "backdrop": 0.25 } })");
    ASSERT_TRUE (p.status.wasOk()) << p.status.getErrorMessage();
    EXPECT_EQ (p.info.author, "me");
    EXPECT_EQ (p.tokens.textPrimary, juce::Colour (0xFF112233));   // #RRGGBB is opaque
    EXPECT_EQ (p.tokens.well, juce::Colour (0x80112233));
    ASSERT_EQ (p.tokens.accents.size(), 2U);
    EXPECT_EQ (p.tokens.accents[1], juce::Colour (0xFF00FF00));
    EXPECT_EQ (p.tokens.gap, 6);
    EXPECT_EQ (p.tokens.innerGap, 3);
    EXPECT_FLOAT_EQ (p.tokens.cardRadius, 20.0F);
    EXPECT_FLOAT_EQ (p.tokens.controlRadius, 4.5F);
    EXPECT_FLOAT_EQ (p.tokens.typeTitle, 22.0F);
    EXPECT_FLOAT_EQ (p.tokens.surfaceOpacity, 0.5F);
    EXPECT_FLOAT_EQ (p.tokens.backdropOpacity, 0.25F);
    EXPECT_FLOAT_EQ (p.tokens.typeBody, ThemeTokens::builtinDark().typeBody);   // untouched keys keep the default
}

TEST (ThemeJsonTest, ParsesMixerMetricsAndMeterBallistics)
{
    const auto p = parse (R"({ "id": "t.mix", "mixer": { "knobHeight": 50, "meterHoldSeconds": 2.5, "meterFloorDb": -60 } })");
    ASSERT_TRUE (p.status.wasOk()) << p.status.getErrorMessage();
    EXPECT_FLOAT_EQ (p.tokens.mixerKnobHeight, 50.0F);
    EXPECT_FLOAT_EQ (p.tokens.meterHoldSeconds, 2.5F);
    EXPECT_FLOAT_EQ (p.tokens.meterFloorDb, -60.0F);
    const auto sized = parse (R"({ "id": "t.sz", "mixer": { "titleHeight": 30, "faderWidth": 50, "curveWidth": 100, "fxBoxWidth": 120,
                                  "fxEnableWidth": 40, "pageButtonWidth": 70, "padX": 4, "padY": 5, "accentDot": 6, "meterHoldTickHeight": 3 } })");
    ASSERT_TRUE (sized.status.wasOk()) << sized.status.getErrorMessage();
    EXPECT_FLOAT_EQ (sized.tokens.mixerTitleHeight, 30.0F);
    EXPECT_FLOAT_EQ (sized.tokens.mixerFxBoxWidth, 120.0F);
    EXPECT_FLOAT_EQ (sized.tokens.mixerAccentDot, 6.0F);
    EXPECT_FLOAT_EQ (sized.tokens.meterHoldTickHeight, 3.0F);
    EXPECT_TRUE (parse (R"({ "id": "t.bad", "mixer": { "faderWidth": -1 } })").status.failed());
    EXPECT_TRUE (parse (R"({ "id": "t.bad", "mixer": { "meterHoldTickHeight": 99 } })").status.failed());
    EXPECT_FLOAT_EQ (p.tokens.mixerCueHeight, ThemeTokens::builtinDark().mixerCueHeight);
    EXPECT_TRUE (parse (R"({ "id": "t.bad", "mixer": { "meterClipLevel": 3 } })").status.failed());
    EXPECT_TRUE (parse (R"({ "id": "t.bad", "mixer": { "meterReleaseSeconds": 0 } })").status.failed());
}

TEST (ThemeJsonTest, ParsesWaveformColoursAndDeckMetrics)
{
    const auto p = parse (R"({ "id": "t.deck", "colours": { "waveLow": "#010203", "waveHigh": "#A0040506" },
                              "deck": { "rowHeight": 22, "pitchWidth": 70, "gridBarAlpha": 0.9, "playheadWidth": 3 } })");
    ASSERT_TRUE (p.status.wasOk()) << p.status.getErrorMessage();
    EXPECT_EQ (p.tokens.waveLow, juce::Colour (0xFF010203));
    EXPECT_EQ (p.tokens.waveHigh, juce::Colour (0xA0040506));
    EXPECT_EQ (p.tokens.waveMid, ThemeTokens::builtinDark().waveMid);
    EXPECT_FLOAT_EQ (p.tokens.deckRowHeight, 22.0F);
    EXPECT_FLOAT_EQ (p.tokens.deckPitchWidth, 70.0F);
    EXPECT_FLOAT_EQ (p.tokens.gridBarAlpha, 0.9F);
    EXPECT_FLOAT_EQ (p.tokens.playheadWidth, 3.0F);
    EXPECT_FLOAT_EQ (p.tokens.deckHeaderHeight, ThemeTokens::builtinDark().deckHeaderHeight);

    EXPECT_TRUE (parse (R"({ "id": "t.bad", "deck": { "gridBarAlpha": 2 } })").status.failed());
    EXPECT_TRUE (parse (R"({ "id": "t.bad", "deck": { "rowHeight": -1 } })").status.failed());
}

TEST (ThemeJsonTest, RejectsBadInputAndFallsBackToDefaultTokens)
{
    const char* bad[] = {
        "{ not json",
        "[1, 2]",
        R"({ "name": "no id" })",
        R"({ "id": "Has Spaces" })",
        R"({ "id": "t", "name": "  " })",
        R"({ "id": "t", "colours": { "well": "red" } })",
        R"({ "id": "t", "colours": { "well": "#12345" } })",
        R"({ "id": "t", "colours": { "well": "#GG0000" } })",
        R"({ "id": "t", "colours": 5 })",
        R"({ "id": "t", "accents": [] })",
        R"({ "id": "t", "accents": ["#FF0000", "nope"] })",
        R"({ "id": "t", "accents": ["#111111","#111111","#111111","#111111","#111111","#111111","#111111","#111111","#111111"] })",
        R"({ "id": "t", "spacing": { "gap": -1 } })",
        R"({ "id": "t", "spacing": { "gap": 12.5 } })",
        R"({ "id": "t", "spacing": { "gap": 500 } })",
        R"({ "id": "t", "radii": { "card": "big" } })",
        R"({ "id": "t", "type": { "title": 2 } })",
        R"({ "id": "t", "translucency": { "surface": 1.5 } })",
    };
    for (const auto* json : bad)
    {
        const auto p = parse (json);
        EXPECT_TRUE (p.status.failed()) << json;
        EXPECT_FALSE (p.status.getErrorMessage().isEmpty()) << json;
        EXPECT_EQ (p.tokens.backdropTop, ThemeTokens::builtinDark().backdropTop) << json;   // fallback, never half-filled
    }
}

TEST (ThemeJsonTest, UnknownKeysWarnButDoNotFail)
{
    const auto p = parse (R"({ "id": "t", "colurs": {}, "colours": { "wel": "#000000" }, "type": { "huge": 30 } })");
    ASSERT_TRUE (p.status.wasOk());
    EXPECT_EQ (p.warnings.size(), 3);
}

TEST (ThemeJsonTest, ColourHexIsStrict)
{
    juce::Colour c (0xFF010203);
    EXPECT_FALSE (parseColourHex ("112233", c));
    EXPECT_FALSE (parseColourHex ("#1122", c));
    EXPECT_FALSE (parseColourHex ("", c));
    EXPECT_EQ (c, juce::Colour (0xFF010203));   // untouched on failure
    EXPECT_TRUE (parseColourHex ("#aAbBcC", c));
    EXPECT_EQ (c, juce::Colour (0xFFAABBCC));
}

TEST (ThemeJsonTest, MissingFileFails)
{
    EXPECT_TRUE (parseThemeFile (juce::File ("/definitely/not/here.json")).status.failed());
}
