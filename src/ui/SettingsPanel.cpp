#include "SettingsPanel.h"

#include "Theme.h"

namespace openmixer::ui
{

namespace { constexpr int kTabRadioGroup = 4701; }

SettingsPanel::SettingsPanel (SettingsServices services)
{
    setWantsKeyboardFocus (true);
    addAndMakeVisible (title);
    addAndMakeVisible (closeButton);
    closeButton.onClick = [this] { close(); };
    addStandardPages (*this, services);
    applyTheme();
}

void SettingsPanel::applyTheme()
{
    title.setFont (theme::font (theme::typeHeading(), true));
    title.setColour (juce::Label::textColourId, theme::textPrimary());
}

void SettingsPanel::addPage (const juce::String& pageTitle, std::unique_ptr<SettingsPage> page)
{
    page->changed = [this] { if (onChanged) onChanged(); };
    auto tab = std::make_unique<juce::TextButton> (pageTitle);
    tab->setClickingTogglesState (true);
    tab->setRadioGroupId (kTabRadioGroup);
    const int index = static_cast<int> (pages.size());
    tab->onClick = [this, index] { showPage (index); };
    addAndMakeVisible (*tab);
    addChildComponent (*page);
    pages.push_back ({ std::move (tab), std::move (page) });
    if (current < 0)
        showPage (0);
    resized();
}

void SettingsPanel::showPage (int index)
{
    if (index < 0 || index >= getNumPages())
        return;
    current = index;
    for (int i = 0; i < getNumPages(); ++i)
    {
        pages[static_cast<size_t> (i)].page->setVisible (i == index);
        pages[static_cast<size_t> (i)].tab->setToggleState (i == index, juce::dontSendNotification);
    }
}

void SettingsPanel::reflectAll()
{
    for (auto& e : pages)
        e.page->reflect();
}

void SettingsPanel::resized()
{
    const int w = std::min (theme::settingsPanelWidth(), getWidth() - 2 * theme::gap());
    const int h = std::min (theme::settingsPanelHeight(), getHeight() - 2 * theme::gap());
    card = getLocalBounds().withSizeKeepingCentre (std::max (0, w), std::max (0, h));

    auto area = card.reduced (theme::pad());
    auto header = area.removeFromTop (theme::settingsTabHeight());
    closeButton.setBounds (header.removeFromRight (theme::settingsButtonWidth()));
    title.setBounds (header);
    area.removeFromTop (theme::innerGap());

    auto tabs = area.removeFromTop (theme::settingsTabHeight());
    const int tabW = pages.empty() ? 0 : std::min (theme::settingsButtonWidth() + theme::pad(), tabs.getWidth() / static_cast<int> (pages.size()));
    for (auto& e : pages)
        e.tab->setBounds (tabs.removeFromLeft (tabW).withTrimmedRight (theme::innerGap()));
    area.removeFromTop (theme::innerGap());
    for (auto& e : pages)
        e.page->setBounds (area);
}

void SettingsPanel::paint (juce::Graphics& g)
{
    g.fillAll (theme::shadow().withAlpha (theme::settingsDimAlpha()));
    g.setColour (theme::popup());
    g.fillRoundedRectangle (card.toFloat(), theme::cardRadius());   // opaque base so the page text stays readable
    theme::paintGlassCard (g, card.toFloat());
}

void SettingsPanel::mouseDown (const juce::MouseEvent& e)
{
    if (! card.contains (e.getPosition()))
        close();
}

bool SettingsPanel::keyPressed (const juce::KeyPress& key)
{
    if (key != juce::KeyPress::escapeKey)
        return false;
    close();
    return true;
}

void addStandardPages (SettingsPanel& panel, const SettingsServices& s)
{
    if (s.audio != nullptr)
        panel.addPage ("Audio", std::make_unique<AudioSettingsPage> (s.prefs, *s.audio, s.latency));
    panel.addPage ("MIDI", std::make_unique<MidiSettingsPage> (s.prefs, s.midiInputs));
    panel.addPage ("Library", std::make_unique<LibrarySettingsPage> (s.prefs, s.library));
    panel.addPage ("Appearance", std::make_unique<AppearanceSettingsPage> (s.prefs, s.themes));
    panel.addPage ("Plugins", std::make_unique<PluginsSettingsPage> (s.prefs, s.loadedPlugins, s.reloadPlugins));
}

} // namespace openmixer::ui
