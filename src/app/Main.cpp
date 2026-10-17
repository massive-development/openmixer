/*
  OpenMixer Main.cpp
  Main entry point for the application
*/

#include <JuceHeader.h>
#include "MainComponent.h"
#include "ui/GlassLookAndFeel.h"
#include "ui/Theme.h"

namespace
{
    // --translucent (or OPENMIXER_TRANSLUCENT=1) lets the desktop show through the glass.
    constexpr float kTranslucentBackdrop = 0.82F;

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

        // Check for smoke test mode
        bool smokeTest = JUCEApplication::getCommandLineParameters().contains ("--smoke");
        #if OPENMIXER_SMOKE
        smokeTest = true;
        #endif

        if (smokeTest)
        {
            // Run smoke test and exit
            Logger::writeToLog ("[Smoke Test] Application starting...");
            
            // Create and test MainComponent
            auto mainComp = std::make_unique<MainComponent>();
            
            // Check it can be created
            if (mainComp != nullptr)
            {
                Logger::writeToLog ("[Smoke Test] MainComponent created successfully");
                Logger::writeToLog ("[Smoke Test] Smoke test PASSED");
                quit();
                return;
            }
        }

        // Normal mode - create main window
        mainWindow = std::make_unique<MainWindow> (getApplicationName(), wantsTranslucentWindow());
    }

    void shutdown() override
    {
        mainWindow.reset();
        LookAndFeel::setDefaultLookAndFeel (nullptr);
    }

private:
    class MainWindow : public DocumentWindow,
                        private Button::Listener
    {
    public:
        // A semi-transparent window needs a transparent background from construction (the peer
        // is created then) and JUCE's own title bar, since native frames can't be see-through.
        MainWindow (String title, bool translucent)
            : DocumentWindow (title,
                              translucent && Desktop::canUseSemiTransparentWindows() ? Colours::transparentBlack
                                                                                       : openmixer::ui::theme::backdropTop,
                              DocumentWindow::allButtons)
        {
            const bool seeThrough = ! isOpaque();
            setUsingNativeTitleBar (! seeThrough);
            auto content = std::make_unique<MainComponent>();
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

    openmixer::ui::GlassLookAndFeel lookAndFeel;   // outlives mainWindow (declared first)
    std::unique_ptr<MainWindow> mainWindow;

    JUCE_DECLARE_NON_COPYABLE (OpenMixerApplication)
};

//==============================================================================
START_JUCE_APPLICATION (OpenMixerApplication)
