// OpenMixer MainLayoutTest - pure layout geometry and JSON form
#include <JuceHeader.h>
#include "ui/MainLayout.h"

#include <gtest/gtest.h>

using namespace openmixer::ui;

TEST (MainLayoutTest, StandardTwoDeckLayoutIsDecksAroundMixerThenLibrary)
{
    const auto spec = LayoutSpec::standard (2);
    ASSERT_EQ (spec.rows.size(), 2u);
    EXPECT_EQ (spec.deckCount(), 2);
    EXPECT_EQ (spec.slotNames().joinIntoString (","), "mixer,library");
    EXPECT_TRUE (spec.rows[0].cells[0].isDeck() && ! spec.rows[0].cells[1].isDeck() && spec.rows[0].cells[2].isDeck());
}

TEST (MainLayoutTest, FourDecksAddARowAndKeepTheSlots)
{
    const auto spec = LayoutSpec::standard (4);
    EXPECT_EQ (spec.deckCount(), 4);
    EXPECT_EQ (spec.rows.size(), 3u);
    EXPECT_EQ (spec.slotNames().size(), 2);
}

TEST (MainLayoutTest, CellsTileTheAreaWithoutOverlap)
{
    const juce::Rectangle<int> area (10, 20, 1000, 700);
    const auto placed = computeLayout (LayoutSpec::standard (2), area, 8);
    ASSERT_EQ (placed.size(), 4u);
    for (size_t i = 0; i < placed.size(); ++i)
    {
        EXPECT_TRUE (area.contains (placed[i].bounds));
        EXPECT_FALSE (placed[i].bounds.isEmpty());
        for (size_t j = i + 1; j < placed.size(); ++j)
            EXPECT_FALSE (placed[i].bounds.intersects (placed[j].bounds));
    }
    EXPECT_EQ (placed[3].bounds.getRight(), area.getRight()) << "the last cell absorbs rounding";
    EXPECT_EQ (placed[3].bounds.getBottom(), area.getBottom());
    EXPECT_GT (placed[1].bounds.getWidth(), 0);
    EXPECT_LT (placed[1].bounds.getWidth(), placed[0].bounds.getWidth()) << "mixer weight is smaller than a deck's";
}

TEST (MainLayoutTest, JsonRoundTripsAndRejectsBadInput)
{
    const auto spec = LayoutSpec::standard (4);
    const auto parsed = LayoutSpec::fromJson (spec.toJson());
    ASSERT_TRUE (parsed.has_value());
    EXPECT_EQ (parsed->deckCount(), 4);
    EXPECT_EQ (parsed->rows.size(), spec.rows.size());
    EXPECT_EQ (parsed->toJson(), spec.toJson());

    for (const char* bad : { "", "{}", "{\"rows\":[]}", "{\"rows\":[{\"cells\":[]}]}", "{\"rows\":[{\"cells\":[{}]}]}",
                             "{\"rows\":[{\"cells\":[{\"deck\":0,\"slot\":\"x\"}]}]}", "{\"rows\":[{\"cells\":[{\"deck\":0,\"weight\":0}]}]}",
                             "{\"rows\":[{\"height\":-1,\"cells\":[{\"deck\":0}]}]}", "not json" })
        EXPECT_FALSE (LayoutSpec::fromJson (bad).has_value()) << bad;
}

TEST (MainLayoutTest, HandWrittenJsonIsAccepted)
{
    const auto spec = LayoutSpec::fromJson (R"({"rows":[{"height":1,"cells":[{"deck":0},{"slot":"mixer","weight":0.5},{"deck":1},{"slot":"plugin.x"}]}]})");
    ASSERT_TRUE (spec.has_value());
    EXPECT_EQ (spec->slotNames().joinIntoString (","), "mixer,plugin.x");
}

TEST (MainLayoutTest, EmptySpecPlacesNothing)
{
    EXPECT_TRUE (computeLayout ({}, { 0, 0, 100, 100 }, 4).empty());
}
