// OpenMixer TestMain - GoogleTest entry point with JUCE GUI/message-manager init
#include <JuceHeader.h>
#include <gtest/gtest.h>

int main (int argc, char** argv)
{
    juce::ScopedJuceInitialiser_GUI gui;
    ::testing::InitGoogleTest (&argc, argv);
    return RUN_ALL_TESTS();
}
