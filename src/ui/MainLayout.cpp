#include "MainLayout.h"

#include <algorithm>
#include <numeric>

namespace openmixer::ui
{

namespace
{
    constexpr float kDeckRowsShare = 0.70F, kMixerWeight = 0.85F;

    CellSpec deckCell (int i) { CellSpec c; c.deck = i; return c; }
    CellSpec slotCell (const char* name, float weight = 1.0F) { CellSpec c; c.slot = name; c.weight = weight; return c; }

    // Splits `length` by weights, `gap` between parts; the last part absorbs rounding.
    std::vector<std::pair<int, int>> split (int length, const std::vector<float>& weights, int gap)
    {
        const float total = std::accumulate (weights.begin(), weights.end(), 0.0F);
        const int n = static_cast<int> (weights.size());
        const float usable = static_cast<float> (std::max (0, length - gap * (n - 1)));
        std::vector<std::pair<int, int>> parts;
        int pos = 0;
        for (int i = 0; i < n; ++i)
        {
            const int size = i == n - 1 ? std::max (0, length - pos)
                                        : static_cast<int> (usable * weights[static_cast<size_t> (i)] / total);
            parts.emplace_back (pos, size);
            pos += size + gap;
        }
        return parts;
    }

    std::optional<CellSpec> parseCell (const juce::var& v)
    {
        if (! v.isObject())
            return std::nullopt;
        CellSpec c;
        c.deck = v.hasProperty ("deck") ? static_cast<int> (v["deck"]) : -1;
        c.slot = v.getProperty ("slot", {}).toString();
        c.weight = static_cast<float> (static_cast<double> (v.getProperty ("weight", 1.0)));
        if ((c.isDeck() == c.slot.isNotEmpty()) || ! (c.weight > 0.0F))
            return std::nullopt;   // exactly one of deck / slot, and a positive weight
        return c;
    }
}

LayoutSpec LayoutSpec::standard (int numDecks)
{
    LayoutSpec spec;
    RowSpec top;
    top.height = kDeckRowsShare;
    top.cells = { deckCell (0), slotCell (kMixerSlot, kMixerWeight), deckCell (1) };
    if (numDecks < 2)
        top.cells = { deckCell (0), slotCell (kMixerSlot, kMixerWeight) };
    spec.rows.push_back (top);
    if (numDecks > 2)
    {
        RowSpec extra;
        extra.height = kDeckRowsShare;
        for (int i = 2; i < numDecks; ++i)
            extra.cells.push_back (deckCell (i));
        spec.rows.push_back (extra);
    }
    RowSpec bottom;
    bottom.height = 1.0F - kDeckRowsShare;
    bottom.cells = { slotCell (kLibrarySlot) };
    spec.rows.push_back (bottom);
    return spec;
}

std::optional<LayoutSpec> LayoutSpec::fromJson (const juce::String& json)
{
    const auto root = juce::JSON::parse (json);
    const auto* rows = root.getProperty ("rows", {}).getArray();
    if (rows == nullptr || rows->isEmpty())
        return std::nullopt;
    LayoutSpec spec;
    for (const auto& r : *rows)
    {
        RowSpec row;
        row.height = static_cast<float> (static_cast<double> (r.getProperty ("height", 1.0)));
        const auto* cells = r.getProperty ("cells", {}).getArray();
        if (cells == nullptr || cells->isEmpty() || ! (row.height > 0.0F))
            return std::nullopt;
        for (const auto& c : *cells)
        {
            const auto cell = parseCell (c);
            if (! cell)
                return std::nullopt;
            row.cells.push_back (*cell);
        }
        spec.rows.push_back (std::move (row));
    }
    return spec;
}

juce::String LayoutSpec::toJson() const
{
    juce::Array<juce::var> rowVars;
    for (const auto& row : rows)
    {
        juce::Array<juce::var> cellVars;
        for (const auto& c : row.cells)
        {
            auto* o = new juce::DynamicObject();   // owned by the juce::var below
            juce::var v (o);
            if (c.isDeck()) o->setProperty ("deck", c.deck); else o->setProperty ("slot", c.slot);
            o->setProperty ("weight", static_cast<double> (c.weight));
            cellVars.add (v);
        }
        auto* r = new juce::DynamicObject();
        juce::var rv (r);
        r->setProperty ("height", static_cast<double> (row.height));
        r->setProperty ("cells", cellVars);
        rowVars.add (rv);
    }
    auto* root = new juce::DynamicObject();
    juce::var rootVar (root);
    root->setProperty ("rows", rowVars);
    return juce::JSON::toString (rootVar);
}

int LayoutSpec::deckCount() const
{
    int n = 0;
    for (const auto& row : rows)
        for (const auto& c : row.cells)
            n = c.isDeck() ? std::max (n, c.deck + 1) : n;
    return n;
}

juce::StringArray LayoutSpec::slotNames() const
{
    juce::StringArray names;
    for (const auto& row : rows)
        for (const auto& c : row.cells)
            if (! c.isDeck())
                names.addIfNotAlreadyThere (c.slot);
    return names;
}

std::vector<PlacedCell> computeLayout (const LayoutSpec& spec, juce::Rectangle<int> area, int gap)
{
    std::vector<PlacedCell> placed;
    std::vector<float> heights;
    for (const auto& row : spec.rows)
        heights.push_back (row.height);
    if (heights.empty())
        return placed;
    const auto rowParts = split (area.getHeight(), heights, gap);
    for (size_t r = 0; r < spec.rows.size(); ++r)
    {
        std::vector<float> weights;
        for (const auto& c : spec.rows[r].cells)
            weights.push_back (c.weight);
        const auto colParts = split (area.getWidth(), weights, gap);
        for (size_t c = 0; c < colParts.size(); ++c)
            placed.push_back ({ spec.rows[r].cells[c],
                                { area.getX() + colParts[c].first, area.getY() + rowParts[r].first, colParts[c].second, rowParts[r].second } });
    }
    return placed;
}

} // namespace openmixer::ui
