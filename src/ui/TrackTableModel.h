// Table model for the library browser: virtualised (JUCE only paints visible rows) and paged (rows are
// fetched kPageSize at a time from a TrackSource and a few pages are cached). Columns: title, artist,
// BPM, key (Camelot), time, rating, genre, date added; clicking a header re-sorts through the source.
// Rows whose key is harmonically compatible with the reference key get a tint. Message thread only.
#pragma once

#include "TrackSource.h"

#include <juce_gui_basics/juce_gui_basics.h>

#include <map>
#include <memory>
#include <optional>

namespace openmixer::ui
{

class TrackTableModel : public juce::TableListBoxModel
{
public:
    static constexpr int kPageSize = 100;
    static constexpr size_t kMaxCachedPages = 6;

    struct Column
    {
        int id;
        const char* title;
        const char* sortField;   // search field name, nullptr = not sortable
        int width;
        bool numeric;
    };
    static const std::vector<Column>& columns();

    // Takes the new source, drops cached pages and counts its rows.
    void setSource (std::unique_ptr<TrackSource> newSource);
    void reload();   // same source, fresh data
    const TrackSource* getSource() const noexcept { return source.get(); }

    // Camelot code to mix with ("" = none); rows with a compatible key are tinted.
    void setReferenceCamelot (const juce::String& camelot);
    bool isCompatible (const library::TrackRecord& track) const;

    const library::TrackRecord* rowAt (int row);
    juce::String cellText (int row, int columnId);
    static juce::String formatDuration (double seconds);   // "4:05"
    static juce::String formatBpm (const std::optional<double>& bpm);
    static juce::String formatAdded (std::int64_t ms);     // "2026-10-18"

    // Called with the track on double-click / Enter; the browser maps it to "load into the first free deck".
    std::function<void (const library::TrackRecord&)> onActivate;
    // A column header was clicked: search field + direction for the next source.
    std::function<void (const juce::String& field, bool descending)> onSortChanged;

    int getNumRows() override { return total; }
    void paintRowBackground (juce::Graphics&, int row, int width, int height, bool selected) override;
    void paintCell (juce::Graphics&, int row, int columnId, int width, int height, bool selected) override;
    void sortOrderChanged (int newSortColumnId, bool forwards) override;
    void cellDoubleClicked (int row, int columnId, const juce::MouseEvent&) override;
    void returnKeyPressed (int lastRowSelected) override;
    juce::var getDragSourceDescription (const juce::SparseSet<int>& selectedRows) override;

private:
    void paintRating (juce::Graphics&, int rating, juce::Rectangle<int> area) const;
    void activate (int row);
    void evictFarthestFrom (int page);

    std::unique_ptr<TrackSource> source;
    std::map<int, std::vector<library::TrackRecord>> pages;   // page index -> rows
    int total = 0;
    std::optional<analysis::MusicalKey> reference;
};

} // namespace openmixer::ui
