#include "MainView.h"

#include "Theme.h"

namespace openmixer::ui
{

void SlotPlaceholder::paint (juce::Graphics& g)
{
    const auto r = getLocalBounds().toFloat().reduced (0.5F);
    theme::paintGlassCard (g, r);
    g.setColour (theme::textMuted());
    g.setFont (theme::font (theme::typeLabel()));
    g.drawText (name, r, juce::Justification::centred);
}

MainView::MainView (DjEngine& engine, ParameterTarget& target, LayoutSpec layout) : spec (std::move (layout))
{
    setOpaque (true);   // paint() fills everything
    // A deck index the engine lacks is skipped rather than crashing a bad layout file.
    decks.resize (static_cast<size_t> (std::min (spec.deckCount(), engine.getNumDecks())));
    for (size_t i = 0; i < decks.size(); ++i)
    {
        decks[i] = std::make_unique<DeckWidget> (DeckBinding { engine, target, static_cast<int> (i) });
        addAndMakeVisible (*decks[i]);
    }
    for (const auto& name : spec.slotNames())
        setSlotContent (name, std::make_unique<SlotPlaceholder> (name));
}

DeckWidget* MainView::getDeck (int index) noexcept
{
    return index >= 0 && index < getNumDecks() ? decks[static_cast<size_t> (index)].get() : nullptr;
}

bool MainView::setSlotContent (const juce::String& slot, std::unique_ptr<juce::Component> content)
{
    if (! spec.slotNames().contains (slot) || content == nullptr)
        return false;
    addAndMakeVisible (*content);
    slots[slot] = std::move (content);   // the old content is removed from the parent by its destructor
    resized();
    return true;
}

juce::Component* MainView::getSlot (const juce::String& slot) noexcept
{
    const auto it = slots.find (slot);
    return it == slots.end() ? nullptr : it->second.get();
}

void MainView::setBackdropOpacity (float opacity)
{
    backdropOpacity = opacity;
    setOpaque (opacity >= 1.0F);
    repaint();
}

void MainView::paint (juce::Graphics& g)
{
    if (! isOpaque())
        g.fillAll (juce::Colours::transparentBlack);
    theme::paintBackdrop (g, getLocalBounds().toFloat(), backdropOpacity);
}

void MainView::resized()
{
    for (const auto& placed : computeLayout (spec, getLocalBounds().reduced (theme::gap()), theme::gap()))
    {
        auto* c = placed.cell.isDeck() ? static_cast<juce::Component*> (getDeck (placed.cell.deck)) : getSlot (placed.cell.slot);
        if (c != nullptr)
            c->setBounds (placed.bounds);
    }
}

} // namespace openmixer::ui
