# Product

What OpenMixer has to be for a working DJ to switch to it. This is the bar every area is measured against: engine, analysis, library, interface and hardware alike. It is not a section of the roadmap.

Items marked **(guess)** are the author's assumptions, open to correction. Everything else was decided by the project owner.

## The person we build for

The first users are the project owner and other working DJs. Not hobbyists trying a free tool: people who play regular sets and already use djay, Serato, Rekordbox or Traktor, and who will give a new program one session. If it feels choppy, looks unfamiliar or feels unpolished, it is no better than a personal tool and they go back.

## The bar: better than what they use now, in the first five minutes

"Works" is not the bar. A competing DJ program has to **look** professional, **read** at a glance in a dark room, and **never** make a DJ think about the software. Concretely:

1. **See the music.** Waveforms are the centre of the screen. Colour carries the spectrum, the beat grid and bar numbers show the phrase, cues and loops are visible on the strip, and the structure of the track (intro, drop, breakdown) can be read before it arrives.
2. **Reach by feel.** The controls a DJ touches mid-mix sit where hands expect them: EQ, filter and gain beside the deck, transport under the waveform, pads large enough to hit without looking, rotary knobs with a visible centre and a level meter beside them.
3. **Find the next track fast.** The library is a performance tool: artwork, BPM and key shown against the playing track so compatible tracks stand out, history, playlists beside the browser, instant search.
4. **Look like a product, and like ours.** The default look is OpenMixer's own: a distinct palette, type and motion that no one would mistake for another program, built on layout conventions a DJ already knows so nothing feels unfamiliar. Depth, hierarchy and restraint: a clear type scale, glow and contrast on active states, real artwork, dark-room-safe colour. Flat grey boxes with default labels are a prototype. Familiarity for people who want it comes from an optional second scheme, not from the default.
5. **Feel smooth.** Look, feel and smoothness are one test. Waveforms scroll at the display's refresh rate with no visible stutter, knobs and faders follow the hand with no lag, nothing jumps or flickers when state changes, and transitions have a short, consistent motion. A dropped frame in a scrolling waveform reads as a dropped beat. Smoothness is measured (frame times with real music, four decks and stems), not judged from a single screenshot.
6. **Be trustworthy.** No glitches over a long set, honest latency numbers, controller maps that work when plugged in, recovery from a bad file without a stop. A DJ forgives a missing feature far sooner than a dropout.

## What "competitive" does not mean

- Not a copy of any product. The clean-room rule (ADR 0001) stands: functional ideas and common layout conventions (waveform on top, EQ beside the deck, crossfader in the middle) are free to use; names, artwork, icons, pixel layouts and manual text are not. We take the *reasons* a layout works, then draw our own.
- Not a feature race. Depth where DJs spend their time (waveform, library, mixing, reliability) beats breadth.
- Not a loss of the engine's guarantees. Real-time safety (no allocation, locks or syscalls on the audio thread) is not traded for polish.

## How this changes the work

- **No area waits for another.** Interface and experience work runs alongside engine work from now on; "UI" is not one wave that opens after the engine is done. See `ROADMAP.md` (tracks).
- **The window is the product, so it is verified as one.** A change to what a DJ sees ships with a screenshot (`tools/screenshots.sh`) looked at by eye, and real music for anything waveform or library related. Logic-layer tests are necessary, not sufficient.
- **Layout and look are allowed to change.** The layout engine, the theme tokens and the controls are tools, not commitments. Replace a layout when a better one is found; keep the tests honest about what must hold (nothing overlaps, nothing is clipped, the minimum window works).
- **Benchmark against the real thing.** Judge a screen by opening a competitor's program with the same track loaded and asking what a DJ would notice first.

## Decisions

- **First user:** the owner and other working DJs.
- **Default look:** brand-specific, unique to OpenMixer, and solid enough that other DJs would use it. A familiar-feeling scheme is optional and later (E8).
- **Deciding qualities:** look, feel and smoothness. Reliability is a precondition, not a selling point.

## Still open

- Which single area, if it were worse than the competition, would end an evaluation: waveform, library or reliability? **(guess: waveform and smoothness first, library second)**
- Target refresh rate and the frame-time budget to hold at four decks with stems. **(guess: the display's refresh rate, no frame over 1.5x the refresh interval over a ten-minute set)**
