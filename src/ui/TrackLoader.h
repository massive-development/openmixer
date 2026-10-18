// Loads library tracks into decks: the file goes to the engine deck, then the deck widget gets the
// track metadata, and the cached waveform + beat grid when analysis already ran. A track that is not
// analysed yet is queued in the prep queue with priority; when the queue reports it done the widget
// is refreshed. Message thread only; the services, engine and view must outlive the loader.
#pragma once

#include "LibraryServices.h"
#include "MainView.h"

#include <optional>
#include <vector>

namespace openmixer::ui
{

class TrackLoader : private LibraryServices::Listener
{
public:
    TrackLoader (DjEngine& engine, MainView& view, LibraryServices& services);
    ~TrackLoader() override;

    juce::Result load (int deck, std::int64_t trackId);
    juce::Result load (int deck, const library::TrackRecord& track);

    std::optional<library::TrackRecord> loadedTrack (int deck) const;
    // Where a double-click should load: an empty idle deck, else any idle deck, else deck 0.
    int suggestedDeck() const;
    // Camelot code of the track playing now (first playing deck), else the first loaded deck; empty if none.
    juce::String referenceCamelot() const;

private:
    struct Slot
    {
        std::optional<library::TrackRecord> track;
        bool awaitingAnalysis = false;
    };
    void apply (int deck, const library::TrackRecord& track, const TrackAnalysis& analysis);
    void prepEntryChanged (const library::PrepEntry& entry) override;
    void refreshFromLibrary (int deck);
    bool validDeck (int deck) const noexcept { return deck >= 0 && deck < static_cast<int> (slots.size()); }

    DjEngine& engine;
    MainView& view;
    LibraryServices& services;
    std::vector<Slot> slots;
};

} // namespace openmixer::ui
