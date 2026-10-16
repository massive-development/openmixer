/*
  OpenMixer MainComponent.cpp
  Main application component implementation
*/

#include <JuceHeader.h>
#include "MainComponent.h"

//==============================================================================
MainComponent::MainComponent()
{
    // Initialize UI
    addAndMakeVisible (titleLabel);
    titleLabel.setText ("OpenMixer", dontSendNotification);
    titleLabel.setFont (juce::Font ("Arial", 48.0f, juce::Font::bold));
    titleLabel.setJustificationType (juce::Justification::centred);
    titleLabel.setColour (juce::Label::textColourId, juce::Colours::white);
    
    addAndMakeVisible (versionLabel);
    versionLabel.setText (ProjectInfo::versionString, dontSendNotification);
    versionLabel.setFont (juce::Font ("Arial", 14.0f, 0));
    versionLabel.setJustificationType (juce::Justification::centred);
    versionLabel.setColour (juce::Label::textColourId, juce::Colours::lightgrey);

    // Without an explicit size the window opens at 0x0
    setSize (900, 600);

    // AudioAppComponent owns its own deviceManager; use it so the audio callbacks actually run
    setAudioChannels (0, 2);
    audioInitialized = deviceManager.getCurrentAudioDevice() != nullptr;
}

MainComponent::~MainComponent()
{
    shutdownAudio();
}

//==============================================================================
void MainComponent::prepareToPlay (int samplesPerBlockExpected, double sampleRate)
{
    // Audio preparation
}

void MainComponent::getNextAudioBlock (const AudioSourceChannelInfo& bufferToFill)
{
    // Silent audio - will be replaced by actual deck audio
    for (int channel = 0; channel < bufferToFill.buffer->getNumChannels(); ++channel)
    {
        bufferToFill.buffer->clear (channel, bufferToFill.startSample, bufferToFill.numSamples);
    }
}

void MainComponent::releaseResources()
{
    // Audio cleanup
}

//==============================================================================
void MainComponent::paint (juce::Graphics& g)
{
    // Dark theme background; child labels draw their own text
    g.fillAll (juce::Colour (0xFF1a1a2e));
}

void MainComponent::resized()
{
    auto bounds = getLocalBounds().reduced (40);
    titleLabel.setBounds (bounds.removeFromTop (60));
    versionLabel.setBounds (bounds.removeFromBottom (30));
}
