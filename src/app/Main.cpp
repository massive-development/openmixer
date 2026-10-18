/*
  OpenMixer Main.cpp
  Main entry point for the application
*/

#include <JuceHeader.h>
#include "MainComponent.h"
#include "ui/GlassLookAndFeel.h"
#include "ui/Theme.h"
#include "ui/ThemeRegistry.h"

namespace
{
    // --translucent (or OPENMIXER_TRANSLUCENT=1) lets the desktop show through the glass.
    constexpr float kTranslucentBackdrop = 0.82F;

    constexpr int kSmokeRunMs = 500;
    constexpr int kSmokeSkipCode = 77;   // matches SKIP_RETURN_CODE of the ctest entry

    // Windows and macOS always have a window server; a Linux session needs X11 or Wayland.
    bool displayAvailable()
    {
       #if JUCE_LINUX || JUCE_BSD
        return SystemStats::getEnvironmentVariable ("DISPLAY", {}).isNotEmpty()
            || SystemStats::getEnvironmentVariable ("WAYLAND_DISPLAY", {}).isNotEmpty();
       #else
        return true;
       #endif
    }

    bool wantsTranslucentWindow()
    {
        return JUCEApplication::getCommandLineParameters().contains ("--translucent")
            || SystemStats::getEnvironmentVariable ("OPENMIXER_TRANSLUCENT", {}) == "1";
    }
}

//==============================================================================
class OpenMixerApplication : public JUCEApplication
{
public:
    OpenMixerApplication() {}

    const String getApplicationName() override       { return ProjectInfo::projectName; }
    const String getApplicationVersion() override    { return ProjectInfo::versionString; }
    bool moreThanOneInstanceAllowed() override       { return false; }

    void initialise (const String& commandLine) override
    {
        LookAndFeel::setDefaultLookAndFeel (&lookAndFeel);
        selectTheme();

        // --smoke: open the main window, run briefly, exit 0 (kSmokeSkipCode when there is no display)
        bool smokeTest = JUCEApplication::getCommandLineParameters().contains ("--smoke");
        #if OPENMIXER_SMOKE
        smokeTest = true;
        #endif

        if (smokeTest && ! displayAvailable())
        {
            Logger::writeToLog ("[Smoke Test] no display available, skipping");
            setApplicationReturnValue (kSmokeSkipCode);
            quit();
            return;
        }

        // Normal mode - create main window
        mainWindow = std::make_unique<MainWindow> (getApplicationName(), wantsTranslucentWindow(), themes);
        if (smokeTest)
            Timer::callAfterDelay (kSmokeRunMs, [this] { finishSmoke(); });
    }

    void shutdown() override
    {
        mainWindow.reset();
        LookAndFeel::setDefaultLookAndFeel (nullptr);
    }

private:
    // The window must have shown and still hold its content after the run, else the smoke test fails.
    void finishSmoke()
    {
        const bool ok = mainWindow != nullptr && mainWindow->isShowing() && mainWindow->getContentComponent() != nullptr;
        Logger::writeToLog (ok ? "[Smoke Test] PASSED" : "[Smoke Test] FAILED: main window is not showing");
        setApplicationReturnValue (ok ? 0 : 1);
        quit();
    }

    // Built-in + user themes, default selected; MainComponent applies the saved theme (and OPENMIXER_THEME).
    void selectTheme()
    {
        themes.loadBuiltIns();
        themes.loadUserDirectory (openmixer::ui::ThemeRegistry::defaultUserDirectory());
        themes.select (openmixer::ui::ThemeRegistry::defaultId());
    }

    class MainWindow : public DocumentWindow,
                        private Button::Listener
    {
    public:
        // A semi-transparent window needs a transparent background from construction (the peer
        // is created then) and JUCE's own title bar, since native frames can't be see-through.
        MainWindow (String title, bool translucent, openmixer::ui::ThemeRegistry& themeRegistry)
            : DocumentWindow (title,
                              translucent && Desktop::canUseSemiTransparentWindows() ? Colours::transparentBlack
                                                                                       : openmixer::ui::theme::backdropTop(),
                              DocumentWindow::allButtons)
        {
            const bool seeThrough = ! isOpaque();
            setUsingNativeTitleBar (! seeThrough);
            auto content = std::make_unique<MainComponent> (themeRegistry);
            if (seeThrough)
                content->setBackdropOpacity (kTranslucentBackdrop);
            setContentOwned (content.release(), true);
            setResizable (true, true);

           #if JUCE_IOS || JUCE_ANDROID
            setFullScreen (true);
           #else
            centreWithSize (getWidth(), getHeight());  // size comes from MainComponent::setSize
           #endif

            setVisible (true);
        }

        void closeButtonPressed() override
        {
            JUCEApplication::getInstance()->systemRequestedQuit();
        }

        void buttonClicked (juce::Button* button) override
        {
        }

    private:
        JUCE_DECLARE_NON_COPYABLE (MainWindow)
    };

    openmixer::ui::ThemeRegistry themes;
    openmixer::ui::GlassLookAndFeel lookAndFeel;   // outlives mainWindow (declared first)
    std::unique_ptr<MainWindow> mainWindow;

    JUCE_DECLARE_NON_COPYABLE (OpenMixerApplication)
};

//==============================================================================
START_JUCE_APPLICATION (OpenMixerApplication)
