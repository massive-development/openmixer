# Alpha gate session (about 45 minutes, Linux, real sound card)

This is the short, ordered version of the alpha gate in issue #128. Run it on a native Linux install with a real sound card and headphones or monitors, on the installed package or `cargo run --release -p openmixer`. Tick a box only after you saw or heard it. Write what you saw next to any failure and open an issue per failure. The longer checklist for optional features is [hardware-checklist.md](hardware-checklist.md).

The Tauri window has the decks, mixer and meters, and Load by dialog or drop. It has the Automix bar and the Record button, and the stems and sampler; it has no library browser, MIDI or cue output yet (see "Known limits" in the release notes). The GPUI window that used to cover them was removed, so those gates (items 6 and 7) wait until the features are ported to the Tauri window; the last commit with GPUI is recorded in the Tauri parity notes.

Use scratch data so your own files stay untouched:

```bash
export OPENMIXER_PREFS=/tmp/om-check/prefs.json
export OPENMIXER_LIBRARY_DB=/tmp/om-check/library.db
mkdir -p /tmp/om-check
```

Have two or three real tracks of different tempo and key ready (WAV, FLAC or MP3).

## 1. Before you listen (5 min)

- [ ] `openmixer --version` prints the version you expect.
- [ ] `openmixer --headless --smoke` exits 0.
- [ ] The window opens with no "mock device" banner. If the banner shows, note the message: the output did not open (#135).
- [ ] The window opens at 1600 x 780 and fits a 1080p display without clipping.

## 2. Two decks to a real device (#135, #128)

- [ ] Load track 1 on deck A and track 2 on deck B with the Load button; then load a third by dropping a file on a deck.
- [ ] Play A. Sound comes out of the device you expect, no clicks, no crackle. Run it 5 minutes and watch for dropouts.
- [ ] While playing, open `top` or the system monitor: CPU is moderate. Note the figure.
- [ ] Real-time priority: `chrt -p $(pidof openmixer)` or `ps -eo pid,cls,rtprio,comm | grep openmixer` shows a real-time class on the audio thread (#244). Note the result either way.
- [ ] Under load (for example `stress-ng --cpu 4` for a minute) the audio does not break up. Note when it does.

## 3. Mixer, EQ and sync by ear

- [ ] Crossfader and channel faders move the right deck; the meters follow.
- [ ] EQ low, mid and high kill their bands; filter sweeps cleanly, no zipper noise.
- [ ] Pitch fader changes speed; key-lock on keeps the pitch steady.
- [ ] Sync two tracks: beats stay locked for a full minute. Nudge and listen for flams.
- [ ] Hot cues and loops: set, trigger, and loop wrap are seamless.
- [ ] Waveform scrolls smoothly (no stutter) with both decks playing.

## 4. Beat grid on real music (#260 changed the phase)

- [ ] On three real tracks (one with a heavy off-beat bass or open hats), the grid sits on the kick, not half a beat late.

## 5. Platform check

- [ ] Quit and restart: nothing crashes, the window state is sane.
- [ ] Unplug the output device while playing, then plug it in again. Note what happens (the app must not crash).

## 6. Stems, Automix, sampler, recording (all four are in the Tauri window) (#131, #132, #147, #148)

Stems are in each deck's Stems tab and the sampler is in the drawer; Automix and recording are in the top bar. The htdemucs model downloads from Settings, Stems. Needs `cargo run --release -p openmixer --features onnx` and the htdemucs model.

- [ ] Stems: separate one real track; listen for seams and clicks, mute and solo each stem.
- [ ] Automix: one transition on real decks, listen for phase, EQ and bass swap.
- [ ] Sampler: 8 pads, latency and clicks, loop latch and gate.
- [ ] Recording: record 30 s of a mix, open the file, check level and no glitches.

## 7. Headphone cue and MIDI (#135, #146)

- [ ] With two devices (once the cue output is in the Tauri window): the cue stream plays on the second device (it failed on WSLg).
- [ ] With a class-compliant MIDI controller: it appears in the input list and the generic map moves the right controls.

## Report

Per failed item: build commit, OS and desktop (X11 or Wayland), sound card, what you heard. Tick the matching boxes in #128 and #283 when done.
