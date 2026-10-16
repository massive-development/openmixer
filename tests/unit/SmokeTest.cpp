// OpenMixer SmokeTest - project metadata sanity check
#include <JuceHeader.h>
#include <gtest/gtest.h>

TEST (SmokeTest, ProjectInfoIsDefined)
{
    EXPECT_TRUE (juce::String (ProjectInfo::projectName).isNotEmpty());
    EXPECT_TRUE (juce::String (ProjectInfo::versionString).isNotEmpty());
}
