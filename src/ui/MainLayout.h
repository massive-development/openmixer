// Data-driven arrangement of the main window: rows of cells, each a deck or a named slot
// (mixer, library, or anything a plugin registers). Pure geometry, no Components, so adding
// decks later is a data change. JSON form:
//   {"rows":[{"height":0.66,"cells":[{"deck":0},{"slot":"mixer","weight":0.4},{"deck":1}]},
//            {"height":0.34,"cells":[{"slot":"library"}]}]}
#pragma once

#include <juce_core/juce_core.h>
#include <juce_graphics/juce_graphics.h>

#include <optional>
#include <vector>

namespace openmixer::ui
{

struct CellSpec
{
    int deck = -1;          // >= 0: a deck widget
    juce::String slot;      // otherwise: a named slot filled by the app or a later task
    float weight = 1.0F;    // share of the row width
    bool isDeck() const noexcept { return deck >= 0; }
};

struct RowSpec
{
    float height = 1.0F;    // share of the window height
    std::vector<CellSpec> cells;
};

struct LayoutSpec
{
    std::vector<RowSpec> rows;

    static constexpr const char* kMixerSlot = "mixer";
    static constexpr const char* kLibrarySlot = "library";

    // Decks flank the mixer on the first row (any further decks get a row below), library at the bottom.
    static LayoutSpec standard (int numDecks);
    static std::optional<LayoutSpec> fromJson (const juce::String& json);
    juce::String toJson() const;

    int deckCount() const;
    juce::StringArray slotNames() const;
};

struct PlacedCell
{
    CellSpec cell;
    juce::Rectangle<int> bounds;
};

// Splits `area` by row heights and cell weights with `gap` between neighbours.
std::vector<PlacedCell> computeLayout (const LayoutSpec& spec, juce::Rectangle<int> area, int gap);

} // namespace openmixer::ui
