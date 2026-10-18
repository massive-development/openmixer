#include "LibraryBrowser.h"

#include "Theme.h"

namespace openmixer::ui
{

namespace
{
    constexpr int kPollMs = 120;   // search debounce, deferred refreshes and reference-key polling share one tick
}

LibraryBrowser::LibraryBrowser (LibraryServices& s, int numDecks) : services (s)
{
    search.setMultiLine (false);
    search.setTextToShowWhenEmpty ("Search: artist:name bpm:120-128 key:~8A -genre:techno sort:-added", theme::textMuted());
    search.onTextChange = [this] { searchPending = true; };
    addAndMakeVisible (search);

    for (int d = 0; d < numDecks; ++d)
    {
        auto& b = loadButtons.emplace_back (std::make_unique<juce::TextButton> ("Load " + juce::String (d + 1)));
        b->setTooltip ("Load the selected track into deck " + juce::String (d + 1));
        b->onClick = [this, d] { loadSelected (d); };
        addAndMakeVisible (*b);
    }
    prepButton.setTooltip ("Analyse the selected tracks (prep queue)");
    prepButton.onClick = [this] { prepSelected(); };
    importButton.setTooltip ("Import an iTunes / Music or Rekordbox XML library");
    importButton.onClick = [this] { chooseImportFile(); };
    addAndMakeVisible (prepButton);
    addAndMakeVisible (importButton);

    model.onActivate = [this] (const library::TrackRecord& t)
    {
        if (onLoadRequested)
            onLoadRequested (defaultDeck ? defaultDeck() : 0, t);
    };
    model.onSortChanged = [this] (const juce::String& field, bool descending)
    {
        sortField = field;
        sortDescending = descending;
        applyQuery();
    };
    for (const auto& c : TrackTableModel::columns())
        table.getHeader().addColumn (c.title, c.id, c.width, 40, -1,
                                     c.sortField != nullptr ? juce::TableHeaderComponent::defaultFlags
                                                            : juce::TableHeaderComponent::visible | juce::TableHeaderComponent::resizable);
    table.setMultipleSelectionEnabled (true);
    table.setColour (juce::ListBox::backgroundColourId, juce::Colours::transparentBlack);
    addAndMakeVisible (table);

    status.setBorderSize ({ 0, theme::innerGap(), 0, 0 });
    addAndMakeVisible (status);
    addAndMakeVisible (sidebar);

    sidebar.onSelected = [this] (const SidebarNode&) { applyQuery(); };
    sidebar.setNodes (buildSidebar (services.getLibrary()));
    sidebar.select (SidebarNode::Kind::AllTracks);
    if (sidebar.getSelected() == nullptr)
        applyQuery();

    services.addListener (this);
    applyTheme();
    startTimer (kPollMs);
}

LibraryBrowser::~LibraryBrowser()
{
    stopTimer();
    services.removeListener (this);
}

void LibraryBrowser::applyTheme()
{
    table.setRowHeight (theme::libraryRowHeight());
    table.setHeaderHeight (theme::libraryHeaderHeight());
    search.applyFontToAllText (theme::font (theme::typeBody()));
    status.setFont (theme::font (theme::typeCaption()));
    for (size_t d = 0; d < loadButtons.size(); ++d)
        loadButtons[d]->setColour (juce::TextButton::buttonColourId, theme::accentFor (static_cast<int> (d)).withAlpha (0.35F));
    setStatus (status.getText(), statusIsError);
}

void LibraryBrowser::setStatus (const juce::String& text, bool isError)
{
    statusIsError = isError;
    status.setColour (juce::Label::textColourId, isError ? theme::meterHigh() : theme::textSecondary());
    status.setText (text, juce::dontSendNotification);
}

void LibraryBrowser::setSearchText (const juce::String& text)
{
    search.setText (text, juce::dontSendNotification);
    searchPending = false;
    applyQuery();
}

void LibraryBrowser::applyQuery()
{
    searchPending = false;
    stale = false;
    const auto* node = sidebar.getSelected();
    if (node != nullptr && node->kind == SidebarNode::Kind::PrepQueue)
        model.setSource (std::make_unique<PrepQueueSource> (services.getLibrary(), services.getPrepQueue()));
    else
        model.setSource (std::make_unique<SearchTrackSource> (services.getSearch(), node != nullptr ? node->scopeQuery() : juce::String(),
                                                              search.getText(), sortField, sortDescending));
    table.updateContent();
    table.repaint();
    const auto error = model.getSource()->error();
    const bool onQueue = node != nullptr && node->kind == SidebarNode::Kind::PrepQueue;
    const auto count = juce::String (model.getNumRows());
    setStatus (error.isNotEmpty() ? error : count + (onQueue ? " in the prep queue" : " tracks"), error.isNotEmpty());
}

void LibraryBrowser::refresh()
{
    sidebar.setNodes (buildSidebar (services.getLibrary()));
    rebuildSidebar = false;
    applyQuery();
}

void LibraryBrowser::prepEntryChanged (const library::PrepEntry& entry)
{
    const auto* node = sidebar.getSelected();
    const bool onQueue = node != nullptr && node->kind == SidebarNode::Kind::PrepQueue;
    if (onQueue || entry.status == library::PrepStatus::Done)   // done: bpm/key columns changed
        stale = true;
}

void LibraryBrowser::flushPending()
{
    if (rebuildSidebar)
        return refresh();
    if (searchPending || stale)
        applyQuery();
    const auto reference = referenceKey ? referenceKey() : juce::String();
    if (reference != shownReference)
    {
        shownReference = reference;
        model.setReferenceCamelot (reference);
        table.repaint();
    }
}

void LibraryBrowser::loadSelected (int deck)
{
    if (! onLoadRequested)
        return;
    if (const auto* t = model.rowAt (table.getSelectedRow()); t != nullptr)
        onLoadRequested (deck, *t);
    else
        setStatus ("Select a track first", true);
}

int LibraryBrowser::prepSelected()
{
    int queued = 0;
    const auto rows = table.getSelectedRows();
    for (int i = 0; i < rows.size(); ++i)
        if (const auto* t = model.rowAt (rows[i]); t != nullptr && services.getPrepQueue().enqueue (t->id).wasOk())
            ++queued;
    setStatus (queued > 0 ? "Queued " + juce::String (queued) + " for analysis" : "Select tracks to analyse first", queued == 0);
    return queued;
}

juce::Result LibraryBrowser::importFrom (const juce::File& xml)
{
    library::ImportReport report;
    const auto result = services.importFile (xml, report);
    if (result.failed())
        setStatus ("Import failed: " + result.getErrorMessage(), true);
    else
        setStatus ("Imported " + juce::String (report.tracksAdded) + " new, " + juce::String (report.tracksMerged) + " merged, "
                       + juce::String (report.playlistsCreated) + " playlists" + (report.errors > 0 ? ", " + juce::String (report.errors) + " problems" : ""),
                   report.errors > 0);
    return result;
}

void LibraryBrowser::chooseImportFile()
{
    if (chooser != nullptr)
        return;   // one dialog at a time
    chooser = std::make_unique<juce::FileChooser> ("Import iTunes / Rekordbox XML", juce::File(), "*.xml");
    chooser->launchAsync (juce::FileBrowserComponent::openMode | juce::FileBrowserComponent::canSelectFiles,
                          [safe = juce::Component::SafePointer<LibraryBrowser> (this)] (const juce::FileChooser& fc)
                          {
                              if (safe == nullptr)
                                  return;
                              const auto file = fc.getResult();
                              safe->chooser.reset();
                              if (file.existsAsFile())
                                  safe->importFrom (file);
                          });
}

void LibraryBrowser::paint (juce::Graphics& g)
{
    theme::paintGlassCard (g, getLocalBounds().toFloat().reduced (0.5F));
    for (const auto& r : { sidebarArea, tableArea })
    {
        g.setColour (theme::well());
        g.fillRoundedRectangle (r.toFloat(), theme::controlRadius());
    }
}

void LibraryBrowser::resized()
{
    const int gap = theme::innerGap();
    auto area = getLocalBounds().reduced (theme::pad());
    auto bar = area.removeFromTop (theme::libraryToolbarHeight());
    const int button = theme::libraryButtonWidth();
    importButton.setBounds (bar.removeFromRight (button));
    bar.removeFromRight (gap);
    prepButton.setBounds (bar.removeFromRight (button));
    bar.removeFromRight (gap);
    for (auto it = loadButtons.rbegin(); it != loadButtons.rend(); ++it)
    {
        (*it)->setBounds (bar.removeFromRight (button));
        bar.removeFromRight (gap);
    }
    search.setBounds (bar);
    area.removeFromTop (gap);

    status.setBounds (area.removeFromBottom (theme::libraryHeaderHeight()));
    sidebarArea = area.removeFromLeft (theme::librarySidebarWidth());
    sidebar.setBounds (sidebarArea.reduced (0, 2));
    area.removeFromLeft (gap);
    tableArea = area;
    table.setBounds (tableArea.reduced (2));
}

} // namespace openmixer::ui
