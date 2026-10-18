#include "TrackLoader.h"

namespace openmixer::ui
{

TrackLoader::TrackLoader (DjEngine& e, MainView& v, LibraryServices& s)
    : engine (e), view (v), services (s), slots (static_cast<size_t> (e.getNumDecks()))
{
    services.addListener (this);
}

TrackLoader::~TrackLoader() { services.removeListener (this); }

juce::Result TrackLoader::load (int deck, std::int64_t trackId)
{
    juce::Result status = juce::Result::ok();
    const auto track = services.getLibrary().tracks().findById (trackId, &status);
    if (status.failed())
        return status;
    if (! track)
        return juce::Result::fail ("unknown track " + juce::String (trackId));
    return load (deck, *track);
}

juce::Result TrackLoader::load (int deck, const library::TrackRecord& track)
{
    if (! validDeck (deck))
        return juce::Result::fail ("no such deck");
    const juce::File file (track.path);
    if (! file.existsAsFile())
        return juce::Result::fail ("file is missing");
    if (! engine.getDeck (deck).load (file))
        return juce::Result::fail ("cannot decode " + file.getFileName());

    const auto found = services.analysisFor (track);
    auto& slot = slots[static_cast<size_t> (deck)];
    slot.track = track;
    slot.awaitingAnalysis = ! found.analysed;
    apply (deck, track, found);
    if (! found.analysed)
        if (auto r = services.getPrepQueue().enqueue (track.id, true); r.failed())
            return r;   // the track is loaded and playable, only the analysis could not be queued
    return juce::Result::ok();
}

void TrackLoader::apply (int deck, const library::TrackRecord& track, const TrackAnalysis& analysis)
{
    auto* widget = view.getDeck (deck);
    if (widget == nullptr)
        return;
    widget->setTrack ({ track.title, track.artist, track.key, track.camelot, track.bpm.value_or (0.0) });
    widget->setWaveform (analysis.waveform);
    widget->setGrid (analysis.grid);
}

void TrackLoader::prepEntryChanged (const library::PrepEntry& entry)
{
    if (entry.status != library::PrepStatus::Done)
        return;
    for (int d = 0; d < static_cast<int> (slots.size()); ++d)
    {
        const auto& slot = slots[static_cast<size_t> (d)];
        if (slot.awaitingAnalysis && slot.track && slot.track->id == entry.trackId)
            refreshFromLibrary (d);
    }
}

// Re-reads the row (analysis wrote bpm/key) and the cache, then updates the deck.
void TrackLoader::refreshFromLibrary (int deck)
{
    auto& slot = slots[static_cast<size_t> (deck)];
    const auto fresh = services.getLibrary().tracks().findById (slot.track->id);
    if (! fresh)
        return;
    const auto found = services.analysisFor (*fresh);
    slot.track = fresh;
    slot.awaitingAnalysis = ! found.analysed;
    apply (deck, *fresh, found);
}

std::optional<library::TrackRecord> TrackLoader::loadedTrack (int deck) const
{
    return validDeck (deck) ? slots[static_cast<size_t> (deck)].track : std::nullopt;
}

int TrackLoader::suggestedDeck() const
{
    int idle = -1;
    for (int d = 0; d < static_cast<int> (slots.size()); ++d)
    {
        if (engine.getDeck (d).isPlaying())
            continue;
        if (! slots[static_cast<size_t> (d)].track)
            return d;
        if (idle < 0)
            idle = d;
    }
    return std::max (0, idle);
}

juce::String TrackLoader::referenceCamelot() const
{
    juce::String firstLoaded;
    for (int d = 0; d < static_cast<int> (slots.size()); ++d)
    {
        const auto& track = slots[static_cast<size_t> (d)].track;
        if (! track || track->camelot.isEmpty())
            continue;
        if (engine.getDeck (d).isPlaying())
            return track->camelot;
        if (firstLoaded.isEmpty())
            firstLoaded = track->camelot;
    }
    return firstLoaded;
}

} // namespace openmixer::ui
