use super::*;
use proptest::prelude::*;

/// Every id the system uses today (engine targets, UI bindings, MIDI fixtures, router tests) plus
/// one canonical form for each grammar branch. Each must print back exactly as written.
const GOLDEN: &[&str] = &[
    // Deck
    "deck.0.play",
    "deck.1.play",
    "deck.2.play",
    "deck.3.play",
    "deck.0.rate",
    "deck.1.rate",
    "deck.0.keylock",
    "deck.1.keylock",
    "deck.0.seek",
    "deck.1.seek",
    "deck.0.loop.exit",
    "deck.0.loop.reloop",
    "deck.0.sync",
    "deck.1.sync",
    "deck.0.sync.master",
    "deck.1.sync.master",
    "deck.0.slip",
    "deck.0.quantize",
    "deck.0.hotcue0",
    "deck.0.hotcue1",
    "deck.0.hotcue2",
    "deck.0.hotcue3",
    "deck.0.hotcue4",
    "deck.0.hotcue5",
    "deck.0.hotcue6",
    "deck.0.hotcue7",
    "deck.0.hotcue2.clear",
    "deck.1.hotcue2.clear",
    "deck.0.hotcue7.clear",
    "deck.0.loop.quantized.0.5",
    "deck.0.loop.quantized.1",
    "deck.0.loop.quantized.2",
    "deck.0.loop.quantized.4",
    "deck.1.loop.quantized.0.25",
    // Mixer channels
    "mixer.ch0.trim",
    "mixer.ch1.trim",
    "mixer.ch2.trim",
    "mixer.ch3.trim",
    "mixer.ch0.fader",
    "mixer.ch1.fader",
    "mixer.ch2.fader",
    "mixer.ch3.fader",
    "mixer.ch0.filter",
    "mixer.ch1.filter",
    "mixer.ch0.pfl",
    "mixer.ch1.pfl",
    "mixer.ch0.xfader",
    "mixer.ch1.xfader",
    "mixer.ch0.eq.low",
    "mixer.ch0.eq.mid",
    "mixer.ch0.eq.high",
    "mixer.ch1.eq.low",
    "mixer.ch1.eq.mid",
    "mixer.ch1.eq.high",
    "mixer.crossfader",
    "mixer.crossfader.curve",
    "mixer.master",
    "mixer.cue.level",
    "mixer.cue.mix",
    // FX racks and units
    "fx.master.bpm",
    "fx.ch0.bpm",
    "fx.ch1.bpm",
    "fx.ch3.bpm",
    "fx.master.fx0.enable",
    "fx.master.fx0.mix",
    "fx.master.fx1.mix",
    "fx.master.fx2.enable",
    "fx.master.fx0.time",
    "fx.master.fx0.feedback",
    "fx.master.fx1.bpm",
    "fx.ch0.fx1.enable",
    "fx.ch0.fx1.mix",
    "fx.ch1.fx2.enable",
    "fx.ch1.fx3.enable",
    "fx.ch2.fx1.mix",
    "fx.ch1.fx1.size",
    "fx.ch0.fx0.wet",
    "fx.ch1.fx999.enable",
    // Sync target
    "sync.leader",
    "sync.mode",
    "sync.0.sync",
    "sync.1.sync",
    "sync.3.sync",
    // Stems (planned grammar)
    "stem.0.0.volume",
    "stem.0.3.mute",
    "stem.1.2.filter",
    "stem.3.1.volume",
    "stem.2.0.mute",
    "stem.0.1.filter",
    // Sampler (planned grammar)
    "sampler.pad0.trigger",
    "sampler.pad7.volume",
    "sampler.pad3.gate",
    "sampler.pad1.pitch",
    "sampler.pad2.trigger",
    "sampler.pad4.trigger",
    "sampler.pad5.gate",
    "sampler.pad6.pitch",
    // Automix (planned grammar)
    "automix.enable",
    "automix.start",
    "automix.cancel",
    "automix.style",
    // Record
    "record.start",
    "record.stop",
    "record.toggle",
    // Plugin and other prefixes
    "plugin.x",
    "test.knob",
    "demo.level",
    "user.mine",
    "stable.x",
    "slow.x",
    "echo.done",
    "fwd.trigger",
    "x.sync",
    "mixerx.y",
    "my-plugin.knob_1",
    "gain.wet.out",
];

/// Each must fail to parse; the variant is pinned for the cases that name it.
const NEGATIVE: &[&str] = &[
    "",
    ".",
    "nonsense",
    "deck",
    "deck.",
    "deck..play",
    "deck.0.",
    "deck.x.play",
    "deck.0.bogus",
    "deck.0.play ",
    "Has Spaces.x",
    "1.sync",
    "-x.y",
    "deck.4.play",
    "deck.99.play",
    "deck.0.hotcue8",
    "deck.0.hotcue8.clear",
    "deck.0.hotcuex.set",
    "deck.0.hotcue1.bogus",
    "deck.0.loop.quantized",
    "deck.0.loop.quantized.",
    "deck.0.loop.quantized.abc",
    "deck.0.loop.quantized.0",
    "deck.0.loop.quantized.0.0",
    "deck.0.loop.quantized..5",
    "deck.0.loop.quantized.1.2.3",
    "deck.0.loop.quantized.-1",
    "deck.0.loop.quantized.1e3",
    "deck.0000.play",
    "mixer",
    "mixer.ch0",
    "mixer.ch4.fader",
    "mixer.ch99.pfl",
    "mixer.ch0.eq.sub",
    "mixer.nonsense",
    "mixer.chx.fader",
    "fx",
    "fx.master",
    "fx.delay.time",
    "fx.ch9.fx0.mix",
    "fx.ch0000.fx0.mix",
    "fx.master.fx1000.mix",
    "fx.master.fx2147483648.mix",
    "fx.master.bpm.x",
    "fx.master.fx0",
    "fx.master.fx0.time.extra",
    "fx.master.nope",
    "fx.bus.fx0.mix",
    "sync",
    "sync.2",
    "sync.4.sync",
    "sync.x.sync",
    "sync..sync",
    "sync.0.1.sync",
    "stem",
    "stem.0.volume",
    "stem.4.0.volume",
    "stem.0.4.volume",
    "stem.0.0.bogus",
    "sampler",
    "sampler.pad8.trigger",
    "sampler.pad.trigger",
    "sampler.pad0.bogus",
    "automix",
    "automix.bogus",
    "automix.enable.x",
    "record",
    "record.bogus",
    "record.start.x",
    "record.Start",
    "Record.start",
];

#[test]
fn golden_ids_round_trip_canonically() {
    assert!(GOLDEN.len() >= 80, "golden table has {} ids", GOLDEN.len());
    for id in GOLDEN {
        let parsed: ParamId = id.parse().unwrap_or_else(|e| panic!("{id}: {e}"));
        assert_eq!(parsed.to_string(), *id);
    }
}

#[test]
fn negative_ids_are_rejected() {
    for id in NEGATIVE {
        assert!(id.parse::<ParamId>().is_err(), "{id:?} should not parse");
    }
}

#[test]
fn error_variants_name_the_failure() {
    assert_eq!("".parse::<ParamId>(), Err(ParamError::Empty));
    assert!(matches!(
        "deck..play".parse::<ParamId>(),
        Err(ParamError::Malformed(_))
    ));
    assert!(matches!(
        "nonsense".parse::<ParamId>(),
        Err(ParamError::Malformed(_))
    ));
    assert!(matches!(
        "deck.0.bogus".parse::<ParamId>(),
        Err(ParamError::Unknown(_))
    ));
    for id in [
        "deck.4.play",
        "stem.0.4.volume",
        "sampler.pad8.trigger",
        "deck.0.hotcue8",
    ] {
        assert!(
            matches!(id.parse::<ParamId>(), Err(ParamError::OutOfRange { .. })),
            "{id}"
        );
    }
}

#[test]
fn parsed_values_carry_the_grammar_fields() {
    assert_eq!(
        "deck.1.hotcue3.clear".parse::<ParamId>(),
        Ok(ParamId::Deck {
            deck: 1,
            p: DeckParam::Hotcue {
                index: 3,
                clear: true
            }
        })
    );
    assert_eq!(
        "fx.ch1.fx2.time".parse::<ParamId>(),
        Ok(ParamId::Fx {
            rack: FxRack::Channel(1),
            unit: Some(2),
            p: FxParam::Effect("time".to_owned())
        })
    );
    assert_eq!(
        "stem.1.2.mute".parse::<ParamId>(),
        Ok(ParamId::Stem {
            deck: 1,
            stem: 2,
            p: StemParam::Mute
        })
    );
    assert_eq!(
        "x.a.b".parse::<ParamId>(),
        Ok(ParamId::Ext {
            prefix: "x".to_owned(),
            rest: "a.b".to_owned()
        })
    );
}

#[test]
fn leading_zeros_parse_but_print_canonically() {
    let parsed: ParamId = "deck.01.hotcue02".parse().expect("valid");
    assert_eq!(parsed.to_string(), "deck.1.hotcue2");
}

fn grammar_like() -> impl Strategy<Value = String> {
    let head = prop::sample::select(vec![
        "deck", "mixer", "fx", "sync", "stem", "sampler", "automix", "record", "plugin",
    ]);
    let segment = prop::sample::select(vec![
        "0",
        "1",
        "3",
        "4",
        "7",
        "8",
        "99",
        "ch0",
        "ch4",
        "fx0",
        "fx1000",
        "pad3",
        "play",
        "rate",
        "hotcue2",
        "clear",
        "loop",
        "quantized",
        "0.5",
        "sync",
        "master",
        "eq",
        "low",
        "bpm",
        "trim",
        "fader",
        "enable",
        "mix",
        "volume",
        "x",
        "",
    ]);
    (head, prop::collection::vec(segment, 0..6)).prop_map(|(head, rest)| {
        std::iter::once(head.to_owned())
            .chain(rest.into_iter().map(str::to_owned))
            .collect::<Vec<_>>()
            .join(".")
    })
}

proptest! {
    #[test]
    fn parse_never_panics_on_arbitrary_text(text in "\\PC*") {
        let _ = text.parse::<ParamId>();
    }

    #[test]
    fn parse_never_panics_on_grammar_like_text(text in grammar_like()) {
        let _ = text.parse::<ParamId>();
    }

    #[test]
    fn display_reparses_to_the_same_value(text in grammar_like()) {
        if let Ok(id) = text.parse::<ParamId>() {
            let again: ParamId = id.to_string().parse().expect("canonical form must parse");
            prop_assert_eq!(again, id);
        }
    }
}

#[test]
fn sub_namespace_parsers_read_the_tail_of_each_root() {
    assert_eq!("play".parse::<DeckParam>(), Ok(DeckParam::Play));
    assert_eq!(
        "hotcue01.clear".parse::<DeckParam>(),
        Ok(DeckParam::Hotcue {
            index: 1,
            clear: true
        })
    );
    assert!("deck.0.play".parse::<DeckParam>().is_err());
    assert!("hotcue8".parse::<DeckParam>().is_err(), "out of range");
    assert!("".parse::<DeckParam>().is_err());
    assert!(".play".parse::<DeckParam>().is_err());
    assert_eq!(
        "ch1.eq.low".parse::<MixerParam>(),
        Ok(MixerParam::Channel {
            ch: 1,
            p: ChannelParam::Eq(EqBand::Low)
        })
    );
    assert_eq!("1.sync".parse::<SyncParam>(), Ok(SyncParam::DeckEnabled(1)));
    assert_eq!(
        "fx1.mix".parse::<FxRackParam>(),
        Ok(FxRackParam {
            unit: Some(1),
            p: FxParam::Mix
        })
    );
    assert_eq!(
        "bpm".parse::<FxRackParam>(),
        Ok(FxRackParam {
            unit: None,
            p: FxParam::Bpm
        })
    );
    assert_eq!(
        "time".parse::<FxParam>(),
        Ok(FxParam::Effect("time".into()))
    );
    assert!(
        "a.b".parse::<FxParam>().is_err(),
        "effect names are one segment"
    );
}

#[test]
fn namespaces_and_reserved_prefixes() {
    for ns in Namespace::ALL {
        assert_eq!(Namespace::from_root(ns.as_str()), Some(ns));
        assert!(is_reserved_prefix(ns.as_str()));
        assert!(is_reserved_prefix(&format!("{}.0", ns.as_str())));
    }
    assert!(!is_reserved_prefix("decks"));
    assert!(!is_reserved_prefix("plugin.x"));
    assert_eq!(
        "fx.ch1.fx2.mix".parse::<ParamId>().unwrap().namespace(),
        Some(Namespace::Fx)
    );
    assert_eq!("plugin.x".parse::<ParamId>().unwrap().namespace(), None);
}

/// Hand-built values that `Display` would print in a form the parser refuses. `validate` must reject
/// each one, and no validated id may fail to print back.
#[test]
fn hand_built_ids_that_do_not_round_trip_fail_validation() {
    let holes = [
        // An FX rack id with no unit can only be a rack tempo; the others print without a unit.
        ParamId::Fx {
            rack: FxRack::Master,
            unit: None,
            p: FxParam::Enable,
        },
        ParamId::Fx {
            rack: FxRack::Master,
            unit: None,
            p: FxParam::Mix,
        },
        ParamId::Fx {
            rack: FxRack::Channel(1),
            unit: None,
            p: FxParam::Effect("feedback".to_owned()),
        },
        // Units run 0 to 999.
        ParamId::Fx {
            rack: FxRack::Master,
            unit: Some(1000),
            p: FxParam::Enable,
        },
        // Deck and channel indices come from the engine's limits.
        ParamId::Deck {
            deck: 4,
            p: DeckParam::Play,
        },
        ParamId::Mixer(MixerParam::Channel {
            ch: 4,
            p: ChannelParam::Fader,
        }),
        ParamId::Deck {
            deck: 0,
            p: DeckParam::Hotcue {
                index: 8,
                clear: false,
            },
        },
        // An effect parameter is one segment: no dot, not empty, and not a word the grammar owns.
        ParamId::Fx {
            rack: FxRack::Master,
            unit: Some(1),
            p: FxParam::Effect("a.b".to_owned()),
        },
        ParamId::Fx {
            rack: FxRack::Master,
            unit: Some(1),
            p: FxParam::Effect(String::new()),
        },
        ParamId::Fx {
            rack: FxRack::Master,
            unit: Some(1),
            p: FxParam::Effect("enable".to_owned()),
        },
        ParamId::Stem {
            deck: 0,
            stem: STEMS_PER_DECK,
            p: StemParam::Mute,
        },
        ParamId::Sampler {
            pad: SAMPLER_PADS,
            p: SamplerParam::Trigger,
        },
        ParamId::Sync(SyncParam::DeckEnabled(MAX_DECKS)),
        ParamId::Ext {
            prefix: "deck".to_owned(),
            rest: "play".to_owned(),
        },
        ParamId::Ext {
            prefix: "plug".to_owned(),
            rest: String::new(),
        },
        ParamId::Ext {
            prefix: "plug".to_owned(),
            rest: "a..b".to_owned(),
        },
    ];
    for id in &holes {
        let validated = id.validate();
        assert!(validated.is_err(), "{id:?} must not validate");
        let printed = id.to_string();
        assert_ne!(
            printed.parse::<ParamId>().as_ref(),
            Ok(id),
            "{printed} must not round-trip, or the rejection is wrong"
        );
    }
}

/// Every id the grammar accepts validates, and the ids that validate print back to themselves.
#[test]
fn parsed_and_validated_ids_round_trip() {
    for text in GOLDEN {
        let id: ParamId = text.parse().expect(text);
        assert_eq!(id.validate(), Ok(()), "{text}");
        assert_eq!(id.to_string().parse::<ParamId>(), Ok(id), "{text}");
    }
    let built = [
        ParamId::Fx {
            rack: FxRack::Master,
            unit: None,
            p: FxParam::Bpm,
        },
        ParamId::Fx {
            rack: FxRack::Channel(3),
            unit: Some(999),
            p: FxParam::Effect("feedback".to_owned()),
        },
        ParamId::Ext {
            prefix: "plug".to_owned(),
            rest: "a.b".to_owned(),
        },
    ];
    for id in built {
        assert_eq!(id.validate(), Ok(()), "{id:?}");
        assert_eq!(id.to_string().parse::<ParamId>(), Ok(id.clone()), "{id:?}");
    }
}

/// `Command::param` only builds a `Param` for a valid id.
#[test]
fn command_param_refuses_invalid_ids() {
    let bad = ParamId::Deck {
        deck: 9,
        p: DeckParam::Play,
    };
    assert!(crate::Command::param(bad, 1.0).is_err());
    let good = ParamId::Deck {
        deck: 1,
        p: DeckParam::Play,
    };
    assert_eq!(
        crate::Command::param(good.clone(), 1.0),
        Ok(crate::Command::Param {
            id: good,
            value: 1.0
        })
    );
}

/// Loop sizes are finite and within (0, MAX_LOOP_BEATS]. The parser and `validate` agree.
#[test]
fn loop_sizes_are_bounded_at_parse_and_validate() {
    for text in [
        "deck.0.loop.quantized.0",
        "deck.0.loop.quantized.4097",
        "deck.0.loop.quantized.99999999999999999999999999",
        "deck.0.loop.quantized.1.2.3",
        "deck.0.loop.quantized.NaN",
    ] {
        assert!(text.parse::<ParamId>().is_err(), "{text} must not parse");
    }
    let top: ParamId = "deck.0.loop.quantized.4096"
        .parse()
        .expect("the bound itself parses");
    assert_eq!(top.validate(), Ok(()));
    for beats in [0.0, -1.0, 4096.5, 1e9, f64::NAN, f64::INFINITY] {
        let id = ParamId::Deck {
            deck: 0,
            p: DeckParam::LoopQuantized(beats),
        };
        assert!(id.validate().is_err(), "{beats} must not validate");
    }
}
