// A path display with "Choose..." and "Default" buttons, used by settings pages (map file, plugin folder).
// The chooser is asynchronous; tests call pick() / useDefault() to take the same paths without a dialog.
#pragma once

#include <juce_gui_basics/juce_gui_basics.h>

#include <memory>

namespace openmixer::ui
{

class PathRow : public juce::Component
{
public:
    PathRow (juce::String chooserTitle, juce::String filePattern);   // empty pattern = choose a folder

    std::function<void (const juce::File&)> onPicked;
    std::function<void()> onDefault;

    void setPath (const juce::String& text, bool isDefault);
    juce::String getPath() const { return path.getText(); }
    void pick (const juce::File& file) { if (onPicked) onPicked (file); }
    void useDefault() { if (onDefault) onDefault(); }
    juce::TextButton& getChooseButton() noexcept { return choose; }
    juce::TextButton& getDefaultButton() noexcept { return reset; }

    void resized() override;
    void lookAndFeelChanged() override { applyTheme(); }

private:
    void applyTheme();
    void openChooser();

    juce::String title, pattern;
    juce::Label path;
    juce::TextButton choose { "Choose..." }, reset { "Default" };
    std::unique_ptr<juce::FileChooser> chooser;
};

} // namespace openmixer::ui
