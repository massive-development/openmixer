/*
  OpenMixer MainComponent.h
  Main application component
*/

#pragma once

#include <JuceHeader.h>

//==============================================================================
class MainComponent : public AudioAppComponent
{
public:
    MainComponent();
    ~MainComponent() override;

    void prepareToPlay (int samplesPerBlockExpected, double sampleRate) override;
    void getNextAudioBlock (const AudioSourceChannelInfo& bufferToFill) override;
    void releaseResources() override;

    void paint (juce::Graphics& g) override;
    void resized() override;

private:
    // UI components
    juce::Label titleLabel;
    juce::Label versionLabel;

    // Audio device state (device manager is inherited from AudioAppComponent)
    bool audioInitialized = false;

    JUCE_DECLARE_NON_COPYABLE (MainComponent)
};
