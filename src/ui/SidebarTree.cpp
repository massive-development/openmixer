#include "SidebarTree.h"

#include "Theme.h"

namespace openmixer::ui
{

namespace
{
    bool sameNode (const SidebarNode& a, const SidebarNode& b)
    {
        return a.kind == b.kind && a.playlistId == b.playlistId && a.path == b.path;
    }
}

// One row; `node == nullptr` is the invisible root that holds the top-level nodes.
class SidebarTree::Item final : public juce::TreeViewItem
{
public:
    Item (SidebarTree& owner, const SidebarNode* n) : tree (owner), node (n)
    {
        if (node == nullptr)
            return;
        for (const auto& child : node->children)
            addSubItem (new Item (tree, &child));   // JUCE: addSubItem takes ownership
    }

    const SidebarNode* getNode() const noexcept { return node; }
    bool mightContainSubItems() override { return node == nullptr || ! node->children.empty() || ! node->isSelectable(); }
    bool canBeSelected() const override { return node != nullptr && node->isSelectable(); }
    int getItemHeight() const override { return theme::librarySidebarRowHeight(); }
    bool customComponentUsesTreeViewMouseHandler() const override { return true; }
    juce::String getUniqueName() const override { return node != nullptr ? node->label + juce::String (node->playlistId) + node->path : "root"; }

    void itemSelectionChanged (bool isNowSelected) override
    {
        if (isNowSelected && node != nullptr)
            tree.itemSelected (*node);
    }

    void paintItem (juce::Graphics& g, int width, int height) override
    {
        if (node == nullptr)
            return;
        const bool heading = ! node->isSelectable();
        const auto area = juce::Rectangle<int> (0, 0, width, height).reduced (theme::innerGap(), 0);
        if (isSelected())
        {
            g.setColour (theme::accentFor (0).withAlpha (0.3F));
            g.fillRoundedRectangle (juce::Rectangle<int> (0, 0, width, height).toFloat().reduced (2.0F, 1.0F), theme::controlRadius());
        }
        g.setColour (heading ? theme::textMuted() : isSelected() ? theme::textPrimary() : theme::textSecondary());
        g.setFont (theme::font (heading ? theme::typeSmall() : theme::typeBody(), heading));
        g.drawText (heading ? node->label.toUpperCase() : node->label, area, juce::Justification::centredLeft, true);
    }

private:
    SidebarTree& tree;
    const SidebarNode* node;
};

SidebarTree::SidebarTree()
{
    tree.setRootItemVisible (false);
    tree.setOpenCloseButtonsVisible (false);
    tree.setDefaultOpenness (true);
    tree.setColour (juce::TreeView::backgroundColourId, juce::Colours::transparentBlack);
    addAndMakeVisible (tree);
}

SidebarTree::~SidebarTree() { tree.setRootItem (nullptr); }

void SidebarTree::setNodes (std::vector<SidebarNode> nodes)
{
    const auto keep = hasSelection ? std::optional<SidebarNode> (selected) : std::nullopt;
    tree.setRootItem (nullptr);
    root.reset();
    model = std::move (nodes);
    root = std::make_unique<Item> (*this, nullptr);
    for (const auto& n : model)
        root->addSubItem (new Item (*this, &n));   // JUCE: addSubItem takes ownership
    tree.setRootItem (root.get());
    tree.setDefaultOpenness (true);
    root->setOpen (true);
    hasSelection = false;
    if (keep)
        select (keep->kind, keep->playlistId, keep->path);
}

const SidebarNode* SidebarTree::getSelected() const noexcept { return hasSelection ? &selected : nullptr; }

void SidebarTree::itemSelected (const SidebarNode& node)
{
    selected = node;
    hasSelection = true;
    if (onSelected)
        onSelected (node);
}

bool SidebarTree::select (SidebarNode::Kind kind, std::int64_t playlistId, const juce::String& path)
{
    SidebarNode wanted;
    wanted.kind = kind;
    wanted.playlistId = playlistId;
    wanted.path = path;
    for (int row = 0; row < tree.getNumRowsInTree(); ++row)
    {
        auto* item = tree.getItemOnRow (row);
        if (item == nullptr || ! item->canBeSelected())
            continue;
        const auto* node = static_cast<Item*> (item)->getNode();   // every item in this tree is an Item
        if (node != nullptr && sameNode (*node, wanted))
        {
            item->setSelected (true, true);
            return true;
        }
    }
    return false;
}

} // namespace openmixer::ui
