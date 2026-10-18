#include "TrackTableModel.h"

#include "ClockFormat.h"
#include "Theme.h"
#include "TrackDrag.h"
#include "analysis/MusicalKey.h"

namespace openmixer::ui
{

const std::vector<TrackTableModel::Column>& TrackTableModel::columns()
{
    static const std::vector<Column> cols {
        { 1, "Title", "title", 260, false },  { 2, "Artist", "artist", 180, false }, { 3, "BPM", "bpm", 60, true },
        { 4, "Key", "key", 52, true },        { 5, "Time", "duration", 56, true },   { 6, "Rating", "rating", 70, false },
        { 7, "Genre", "genre", 110, false },  { 8, "Added", "added", 92, false } };
    return cols;
}

void TrackTableModel::setSource (std::unique_ptr<TrackSource> newSource)
{
    source = std::move (newSource);
    reload();
}

void TrackTableModel::reload()
{
    pages.clear();
    total = source != nullptr ? source->count() : 0;
}

void TrackTableModel::setReferenceCamelot (const juce::String& camelot)
{
    reference = camelot.isEmpty() ? std::nullopt : analysis::MusicalKey::fromCamelot (camelot);
}

bool TrackTableModel::isCompatible (const library::TrackRecord& track) const
{
    if (! reference || track.camelot.isEmpty())
        return false;
    const auto key = analysis::MusicalKey::fromCamelot (track.camelot);
    return key && analysis::isHarmonicallyCompatible (*reference, *key);
}

// Scrolling is local, so the page farthest from the one being read is the least useful.
void TrackTableModel::evictFarthestFrom (int page)
{
    auto farthest = pages.begin();
    for (auto it = pages.begin(); it != pages.end(); ++it)
        if (std::abs (it->first - page) > std::abs (farthest->first - page))
            farthest = it;
    pages.erase (farthest);
}

const library::TrackRecord* TrackTableModel::rowAt (int row)
{
    if (source == nullptr || row < 0 || row >= total)
        return nullptr;
    const int page = row / kPageSize;
    auto it = pages.find (page);
    if (it == pages.end())
    {
        if (pages.size() >= kMaxCachedPages)
            evictFarthestFrom (page);
        it = pages.emplace (page, source->fetch (page * kPageSize, kPageSize)).first;
    }
    const auto index = static_cast<size_t> (row % kPageSize);
    return index < it->second.size() ? &it->second[index] : nullptr;
}

juce::String TrackTableModel::formatDuration (double seconds) { return formatClock (seconds, false); }

juce::String TrackTableModel::formatBpm (const std::optional<double>& bpm)
{
    return bpm ? juce::String (*bpm, 1) : juce::String ("--");
}

juce::String TrackTableModel::formatAdded (std::int64_t ms)
{
    return ms > 0 ? juce::Time (ms).formatted ("%Y-%m-%d") : juce::String();
}

juce::String TrackTableModel::cellText (int row, int columnId)
{
    const auto* t = rowAt (row);
    if (t == nullptr)
        return {};
    switch (columnId)
    {
        case 1: return t->title.isNotEmpty() ? t->title : juce::File (t->path).getFileNameWithoutExtension();
        case 2: return t->artist;
        case 3: return formatBpm (t->bpm);
        case 4: return t->camelot.isNotEmpty() ? t->camelot : t->key;
        case 5: return formatDuration (t->durationSec);
        case 6: return juce::String (t->rating);
        case 7: return t->genre;
        case 8: return formatAdded (t->dateAddedMs);
        default: return {};
    }
}

void TrackTableModel::paintRowBackground (juce::Graphics& g, int row, int width, int height, bool selected)
{
    const juce::Rectangle<int> r (0, 0, width, height);
    if (selected)
        g.setColour (theme::accentFor (0).withAlpha (0.3F));
    else if (const auto* t = rowAt (row); t != nullptr && isCompatible (*t))
        g.setColour (theme::meterLow().withAlpha (theme::libraryCompatAlpha()));
    else
        g.setColour (theme::textPrimary().withAlpha (row % 2 == 0 ? theme::libraryStripeAlpha() : 0.0F));
    g.fillRect (r);
}

void TrackTableModel::paintRating (juce::Graphics& g, int rating, juce::Rectangle<int> area) const
{
    const float dot = theme::libraryRatingDot();
    const float step = dot * 1.6F;
    auto x = static_cast<float> (area.getX());
    const float y = static_cast<float> (area.getCentreY()) - dot * 0.5F;
    for (int i = 0; i < 5; ++i, x += step)
    {
        g.setColour (i < rating ? theme::accentFor (0) : theme::textMuted().withAlpha (0.35F));
        g.fillEllipse (x, y, dot, dot);
    }
}

void TrackTableModel::paintCell (juce::Graphics& g, int row, int columnId, int width, int height, bool selected)
{
    const auto* t = rowAt (row);
    if (t == nullptr)
        return;
    const juce::Rectangle<int> area (4, 0, width - 8, height);
    if (columnId == 6)
        return paintRating (g, t->rating, area);
    const bool numeric = columnId == 3 || columnId == 4 || columnId == 5;
    const bool compatible = columnId == 4 && isCompatible (*t);
    g.setColour (compatible ? theme::meterLow() : selected || columnId == 1 ? theme::textPrimary() : theme::textSecondary());
    g.setFont (numeric ? theme::monoFont (theme::typeBody()) : theme::font (theme::typeBody(), columnId == 1));
    g.drawText (cellText (row, columnId), area, numeric ? juce::Justification::centredRight : juce::Justification::centredLeft, true);
}

void TrackTableModel::sortOrderChanged (int newSortColumnId, bool forwards)
{
    for (const auto& c : columns())
        if (c.id == newSortColumnId && c.sortField != nullptr && onSortChanged)
            return onSortChanged (c.sortField, ! forwards);
}

void TrackTableModel::activate (int row)
{
    if (const auto* t = rowAt (row); t != nullptr && onActivate)
        onActivate (*t);
}

void TrackTableModel::cellDoubleClicked (int row, int, const juce::MouseEvent&) { activate (row); }
void TrackTableModel::returnKeyPressed (int lastRowSelected) { activate (lastRowSelected); }

juce::var TrackTableModel::getDragSourceDescription (const juce::SparseSet<int>& selectedRows)
{
    if (selectedRows.isEmpty())
        return {};
    const auto* t = rowAt (selectedRows[0]);
    return t != nullptr ? trackdrag::describe (t->id) : juce::var();
}

} // namespace openmixer::ui
