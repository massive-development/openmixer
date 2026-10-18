// OpenMixer FxPanelTest - the FX panel builds its controls from effect descriptors and routes fx.* ids
// (needs the GUI init from TestMain)
#include <JuceHeader.h>
#include "MixerTestUtil.h"
#include "ui/FxPanel.h"

#include <gtest/gtest.h>

using namespace openmixer;
using namespace openmixer::ui;
using mixertest::Rig;

namespace
{
int unitNamed (fx::FxRack& rack, std::string_view name) { return rack.findUnit (name); }
}

TEST (FxPanelTest, ScopeButtonsCoverMasterAndEveryChannel)
{
    Rig rig (4);
    FxPanel panel (rig.binding());
    panel.setSize (420, 400);
    EXPECT_EQ (panel.getNumScopes(), 5);
    EXPECT_EQ (panel.getScopeButton (0).getButtonText(), "Master");
    EXPECT_EQ (panel.getScopeButton (3).getButtonText(), "Ch 3");
    EXPECT_EQ (panel.getScope(), 0);
    panel.getScopeButton (3).onClick();
    EXPECT_EQ (panel.getScope(), 3);
}

TEST (FxPanelTest, UnitListComesFromTheRackAndCoversEveryEffect)
{
    Rig rig;
    FxPanel panel (rig.binding());
    panel.setSize (420, 400);
    auto& rack = rig.engine.getFx().getMasterRack();
    EXPECT_EQ (panel.getUnitBox().getNumItems(), rack.getNumUnits());
    for (int i = 0; i < rack.getNumUnits(); ++i)
        EXPECT_EQ (panel.getUnitBox().getItemText (i).toLowerCase().toStdString(), std::string (rack.getUnit (i)->getEffect().getName()));
}

TEST (FxPanelTest, ParamKnobsAreBuiltFromTheEffectDescriptors)
{
    Rig rig;
    FxPanel panel (rig.binding());
    panel.setSize (420, 400);
    auto& rack = rig.engine.getFx().getMasterRack();

    for (int u = 0; u < rack.getNumUnits(); ++u)
    {
        panel.setUnit (u);
        const auto& effect = rack.getUnit (u)->getEffect();
        ASSERT_EQ (panel.getNumParamKnobs(), effect.getNumParams()) << effect.getName();
        for (int p = 0; p < effect.getNumParams(); ++p)
        {
            const auto& spec = effect.getParamSpec (p);
            auto& slider = panel.getParamKnob (p).getSlider();
            EXPECT_NEAR (slider.getMinimum(), spec.minValue, 1e-6) << effect.getName() << "." << spec.name;
            EXPECT_NEAR (slider.getMaximum(), spec.maxValue, 1e-6);
            EXPECT_NEAR (panel.getParamKnob (p).getValue(), effect.getParamTarget (p), 1e-4) << "starts at the engine value";
            EXPECT_FALSE (panel.getParamKnob (p).getCaption().getText().isEmpty());
            EXPECT_FALSE (panel.getParamKnob (p).getBounds().isEmpty());
        }
    }
}

TEST (FxPanelTest, ParamKnobsRouteTheirFxIds)
{
    Rig rig;
    FxPanel panel (rig.binding());
    panel.setSize (420, 400);
    auto& rack = rig.engine.getFx().getMasterRack();
    const int echo = unitNamed (rack, "echo");
    ASSERT_GE (echo, 0);
    panel.setUnit (echo);

    const int feedback = rack.getUnit (echo)->getEffect().findParam ("feedback");
    ASSERT_GE (feedback, 0);
    panel.getParamKnob (feedback).getSlider().setValue (0.8, juce::sendNotificationSync);
    const auto id = "fx.master.fx" + juce::String (echo) + ".feedback";
    EXPECT_TRUE (rig.recorder.saw (id, 0.8));
    EXPECT_NEAR (rack.getUnit (echo)->getEffect().getParamTarget (feedback), 0.8F, 1e-6F);
}

TEST (FxPanelTest, EveryParameterOfEveryEffectRoutesOnEveryRack)
{
    Rig rig;
    FxPanel panel (rig.binding());
    panel.setSize (420, 400);
    for (int scope = 0; scope < panel.getNumScopes(); ++scope)
    {
        panel.setScope (scope);
        auto& rack = scope == 0 ? rig.engine.getFx().getMasterRack() : rig.engine.getFx().getChannelRack (scope - 1);
        const juce::String name = scope == 0 ? juce::String ("master") : "ch" + juce::String (scope - 1);
        for (int u = 0; u < rack.getNumUnits(); ++u)
        {
            panel.setUnit (u);
            const auto& effect = rack.getUnit (u)->getEffect();
            for (int p = 0; p < effect.getNumParams(); ++p)
            {
                const auto& spec = effect.getParamSpec (p);
                double target = spec.minValue + 0.5 * (static_cast<double> (spec.maxValue) - spec.minValue);
                if (std::abs (target - effect.getParamTarget (p)) < 0.05)
                    target = spec.maxValue;   // a knob already at the value would not send
                panel.getParamKnob (p).getSlider().setValue (target, juce::sendNotificationSync);
                const auto id = "fx." + name + ".fx" + juce::String (u) + "." + juce::String (std::string (spec.name));
                EXPECT_TRUE (rig.recorder.sawId (id)) << id;
                EXPECT_NEAR (effect.getParamTarget (p), static_cast<float> (target), 0.01F) << id;
            }
        }
    }
}

TEST (FxPanelTest, EnableAndMixRouteAndMirrorTheUnit)
{
    Rig rig;
    FxPanel panel (rig.binding());
    panel.setSize (420, 400);
    panel.getScopeButton (2).onClick();   // channel 1
    panel.setUnit (1);
    auto& unit = *rig.engine.getFx().getChannelRack (1).getUnit (1);

    panel.getEnable().onClick();
    EXPECT_TRUE (rig.recorder.saw ("fx.ch1.fx1.enable", 1.0));
    EXPECT_TRUE (unit.isEnabled());
    EXPECT_TRUE (panel.getEnable().getToggleState());
    panel.getEnable().onClick();
    EXPECT_TRUE (rig.recorder.saw ("fx.ch1.fx1.enable", 0.0));
    EXPECT_FALSE (unit.isEnabled());

    panel.getMix().getSlider().setValue (0.35, juce::sendNotificationSync);
    EXPECT_TRUE (rig.recorder.saw ("fx.ch1.fx1.mix", 0.35));
    EXPECT_NEAR (unit.getMix(), 0.35F, 1e-6F);
}

TEST (FxPanelTest, RefreshMirrorsExternalChangesWithoutEchoing)
{
    Rig rig;
    FxPanel panel (rig.binding());
    panel.setSize (420, 400);
    panel.setUnit (0);
    rig.engine.getFx().setParameter ("master.fx0.mix", 0.2);
    rig.engine.getFx().setParameter ("master.fx0.enable", 1.0);
    rig.recorder.calls.clear();
    panel.refresh();
    EXPECT_NEAR (panel.getMix().getValue(), 0.2, 1e-6);
    EXPECT_TRUE (panel.getEnable().getToggleState());
    EXPECT_TRUE (rig.recorder.calls.empty());
}

TEST (FxPanelTest, BeatReadoutFollowsTheRackTempo)
{
    Rig rig;
    FxPanel panel (rig.binding());
    panel.setSize (420, 400);
    EXPECT_TRUE (panel.getBeatText().contains ("120.0 BPM")) << panel.getBeatText();
    EXPECT_TRUE (panel.getBeatText().contains ("500 ms")) << panel.getBeatText();

    rig.engine.getFx().setParameter ("master.bpm", 150.0);
    panel.refresh();
    EXPECT_TRUE (panel.getBeatText().contains ("150.0 BPM")) << panel.getBeatText();
    EXPECT_TRUE (panel.getBeatText().contains ("400 ms")) << panel.getBeatText();
}

TEST (FxPanelTest, ScopesRememberTheirOwnUnit)
{
    Rig rig;
    FxPanel panel (rig.binding());
    panel.setSize (420, 400);
    panel.setUnit (3);
    panel.setScope (1);
    EXPECT_EQ (panel.getUnit(), 0);
    panel.setUnit (5);
    panel.setScope (0);
    EXPECT_EQ (panel.getUnit(), 3);
    EXPECT_EQ (panel.getUnitBox().getSelectedId(), 4);
}

TEST (FxPanelTest, ControlsStayInsideThePanelAndDoNotOverlap)
{
    Rig rig;
    FxPanel panel (rig.binding());
    for (auto [w, h] : { std::pair { 420, 400 }, std::pair { 560, 500 } })
    {
        panel.setSize (w, h);
        for (int u = 0; u < panel.getUnitBox().getNumItems(); ++u)
        {
            panel.setUnit (u);
            std::vector<juce::Component*> kids;
            for (auto* k : panel.getChildren())
                if (k->isVisible())
                    kids.push_back (k);
            for (size_t i = 0; i < kids.size(); ++i)
            {
                EXPECT_TRUE (panel.getLocalBounds().contains (kids[i]->getBounds()) && ! kids[i]->getBounds().isEmpty());
                for (size_t j = i + 1; j < kids.size(); ++j)
                    EXPECT_FALSE (kids[i]->getBounds().intersects (kids[j]->getBounds())) << "unit " << u << ": " << i << "/" << j;
            }
        }
    }
}

TEST (FxPanelTest, AnEmptyRackHasNoUnitSoTheEnableButtonMustNotAssumeOne)
{
    fx::FxRack empty (std::vector<std::string> {});
    EXPECT_EQ (empty.getNumUnits(), 0);
    EXPECT_EQ (empty.getUnit (0), nullptr) << "FxPanel::unit() is null for such a rack and every user guards it";
}

TEST (FxPanelTest, BuildsControlsForAPluginRegisteredEffect)
{
    // A rack built from an extended factory: the panel only needs descriptors, not effect knowledge.
    auto factory = fx::FxFactory::withBuiltIns();
    ASSERT_TRUE (factory.registerEffect ("mygate", [] { return fx::FxFactory::withBuiltIns().create ("gate"); }));
    fx::FxRack rack ({ "mygate" }, factory);
    ASSERT_EQ (rack.getNumUnits(), 1);
    const auto& effect = rack.getUnit (0)->getEffect();
    EXPECT_GT (effect.getNumParams(), 0);
    for (int p = 0; p < effect.getNumParams(); ++p)
        EXPECT_FALSE (effect.getParamSpec (p).name.empty());
}
