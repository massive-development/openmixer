// Library browser (the "library" slot): toolbar (search box using the query syntax, load buttons per deck,
// add to prep queue, import iTunes/Rekordbox XML), sidebar tree and the paged, sortable track table.
// It owns no data: rows come from LibraryServices (message-thread connection). Loading a track is a
// callback (onLoadRequested) so the app decides what "load" means (TrackLoader). Message thread only.
#pragma once

#include "LibraryServices.h"
#include "SidebarTree.h"
#include "TrackTableModel.h"

#include <juce_gui_basics/juce_gui_basics.h>

#include <memory>

namespace openmixer::ui
{

class LibraryBrowser : public juce::Component, private LibraryServices::Listener, private juce::Timer
{
public:
    LibraryBrowser (LibraryServices& services, int numDecks);
    ~LibraryBrowser() override;

    // deck = target deck index; the browser never touches decks itself.
    // Returns the load outcome; a failure is shown in the status line.
    std::function<juce::Result (int deck, const library::TrackRecord& track)> onLoadRequested;
    // Which deck double-click / Enter loads into (default 0).
    std::function<int()> defaultDeck;
    // Camelot code to highlight compatible keys against (default: none).
    std::function<juce::String()> referenceKey;

    void setSearchText (const juce::String& text);   // applies immediately (typing is debounced)
    void flushPending();                             // applies a debounced search / deferred refresh now (the timer calls it)
    void refresh();                                  // rebuild sidebar and re-query
    juce::Result requestLoad (int deck, const library::TrackRecord& track);   // onLoadRequested + status line
    void showResult (const juce::Result& result, const library::TrackRecord& track);   // for loads started elsewhere (drops)
    void loadSelected (int deck);                    // first selected row -> onLoadRequested
    int prepSelected();                              // queues every selected row; returns how many
    juce::Result importFrom (const juce::File& xml); // iTunes / Rekordbox XML, summary goes to the status line

    juce::TextEditor& getSearchBox() noexcept { return search; }
    juce::TableListBox& getTable() noexcept { return table; }
    TrackTableModel& getModel() noexcept { return model; }
    SidebarTree& getSidebar() noexcept { return sidebar; }
    juce::TextButton& getLoadButton (int deck) { return *loadButtons[static_cast<size_t> (deck)]; }
    juce::TextButton& getPrepButton() noexcept { return prepButton; }
    juce::TextButton& getImportButton() noexcept { return importButton; }
    const juce::Label& getStatusLabel() const noexcept { return status; }

    void paint (juce::Graphics& g) override;
    void resized() override;
    void lookAndFeelChanged() override { applyTheme(); }

private:
    void applyTheme();
    void applyQuery();
    void setStatus (const juce::String& text, bool isError);
    void chooseImportFile();
    void libraryChanged() override { stale = true; rebuildSidebar = true; }
    void prepEntryChanged (const library::PrepEntry& entry) override;
    void timerCallback() override { flushPending(); }

    LibraryServices& services;
    TrackTableModel model;
    juce::TextEditor search;
    std::vector<std::unique_ptr<juce::TextButton>> loadButtons;
    juce::TextButton prepButton { "Prep" }, importButton { "Import..." };
    SidebarTree sidebar;
    juce::TableListBox table { "tracks", &model };
    juce::Label status;
    std::unique_ptr<juce::FileChooser> chooser;
    juce::Rectangle<int> sidebarArea, tableArea;
    juce::String sortField;
    bool sortDescending = false;
    bool statusIsError = false;
    bool searchPending = false, stale = false, rebuildSidebar = false;
    juce::String shownReference;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR (LibraryBrowser)
};

} // namespace openmixer::ui
