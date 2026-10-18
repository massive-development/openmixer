// Sidebar tree of the library browser (All tracks / playlists / watch folders / prep queue), drawn
// with theme tokens. Group rows are headings; leaves call onSelected. Message thread only.
#pragma once

#include "SidebarModel.h"

#include <juce_gui_basics/juce_gui_basics.h>

#include <functional>
#include <memory>

namespace openmixer::ui
{

class SidebarTree : public juce::Component
{
public:
    SidebarTree();
    ~SidebarTree() override;

    void setNodes (std::vector<SidebarNode> nodes);   // rebuilds the rows, keeping the selection when the node still exists
    const std::vector<SidebarNode>& getNodes() const noexcept { return model; }
    const SidebarNode* getSelected() const noexcept;
    bool select (SidebarNode::Kind kind, std::int64_t playlistId = 0, const juce::String& path = {});
    int getNumRows() const { return tree.getNumRowsInTree(); }

    std::function<void (const SidebarNode&)> onSelected;

    void resized() override { tree.setBounds (getLocalBounds()); }

private:
    class Item;
    friend class Item;
    void itemSelected (const SidebarNode& node);

    juce::TreeView tree;
    std::vector<SidebarNode> model;
    std::unique_ptr<Item> root;
    SidebarNode selected;
    bool hasSelection = false;
};

} // namespace openmixer::ui
