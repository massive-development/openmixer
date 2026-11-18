#!/usr/bin/env python3
"""Drives the web window (ADR 0009) with real X input and checks each interaction against the engine.

Usage: tools/ui-drive.py [--only GROUP] [--verbose]

Linux and X11 only (WSLg works); Python standard library only. It never builds: it starts the
already-built OPENMIXER_BIN (default target/debug/openmixer, built with --features gui-tauri) with
--ui=tauri --mock-device --ui-drive-out=<temp>/drive.json and WAYLAND_DISPLAY empty, reads the
controls' rectangles and specs from that file and the engine's state from <temp>/drive.json.state
(10 Hz), and presses, drags, scrolls and types with XTest at the controls' screen coordinates.

It moves the real pointer and types real keys on DISPLAY: keep your hands off the mouse for the
two and a half minutes it runs. Before every press it checks that the window under the pointer is
the app's; every pressed button and key is released in a finally block, and the app it started is
always stopped by pid.

One line per check: PASS|FAIL  name  expected  actual. The last line is the summary; the exit code
is 1 when a check failed or the run aborted (no display, no window, the pointer over another
window), 2 when it did not start (not Linux, no binary, another openmixer running).
"""

import argparse
import ctypes
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time

# The knob's whole sweep is this many CSS px of vertical drag (web/src/lib/controls/drag.ts).
KNOB_TRAVEL_PX = 160
# Shift at the press divides a knob drag by this (drag.ts, FINE_DIVISOR).
FINE_DIVISOR = 10
# An arrow moves a slider 1% of its travel, Shift or a page key 10% (controls/keys.ts).
STEP, BIG_STEP = 0.01, 0.1
# The lanes show this many seconds at start; one wheel notch zooms by ZOOM_STEP (waveform/view.ts).
DEFAULT_VISIBLE_SECONDS, ZOOM_STEP = 8.0, 1.15
# A lane's long arrow step in beats (waveform/lanekeys.ts).
LONG_STEP_BEATS = 4
# The state file is rewritten at 10 Hz; a read after this long sees the engine after an action.
STATE_PERIOD_S = 0.1
START_TIMEOUT_S = 60.0
MOVE_STEP_PX = 4

KEYSYMS = {
    "Up": 0xFF52, "Down": 0xFF54, "Left": 0xFF51, "Right": 0xFF53, "Home": 0xFF50,
    "End": 0xFF57, "Delete": 0xFFFF, "space": 0x20, "Tab": 0xFF09, "Shift_L": 0xFFE1,
    "KP_Add": 0xFFAB, "minus": 0x2D, "0": 0x30, "equal": 0x3D,
}


class DriveAbort(Exception):
    """The run cannot go on safely (no window, the pointer is over another window)."""


class Interrupted(Exception):
    """Another input moved the pointer during a group (a hand on the mouse): its checks are void."""


# ---------------------------------------------------------------------------------------------
# X input through libX11 and libXtst.


class XInput:
    """XTest pointer and key events, with every press tracked so `release_all` can undo it."""

    def __init__(self):
        self.x11 = ctypes.CDLL("libX11.so.6")
        self.xt = ctypes.CDLL("libXtst.so.6")
        self._signatures()
        self.display = self.x11.XOpenDisplay(None)
        if not self.display:
            raise DriveAbort("cannot open the X display (is DISPLAY set?)")
        if not self._has_xtest():
            raise DriveAbort("the X server has no XTest extension")
        self.root = self.x11.XDefaultRootWindow(self.display)
        self.buttons = set()
        self.keys = set()

    def _signatures(self):
        x, t = self.x11, self.xt
        vp, ul, i, ui = ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_uint
        x.XOpenDisplay.restype, x.XOpenDisplay.argtypes = vp, [ctypes.c_char_p]
        x.XDefaultRootWindow.restype, x.XDefaultRootWindow.argtypes = ul, [vp]
        x.XFlush.argtypes = [vp]
        x.XSync.argtypes = [vp, i]
        x.XCloseDisplay.argtypes = [vp]
        x.XKeysymToKeycode.restype, x.XKeysymToKeycode.argtypes = ctypes.c_ubyte, [vp, ul]
        x.XQueryPointer.restype = i
        x.XQueryPointer.argtypes = [vp, ul] + [vp] * 7
        x.XRaiseWindow.argtypes = [vp, ul]
        x.XSetInputFocus.argtypes = [vp, ul, i, ul]
        x.XGetInputFocus.argtypes = [vp, vp, vp]
        t.XTestQueryExtension.argtypes = [vp] + [ctypes.POINTER(i)] * 4
        t.XTestFakeMotionEvent.argtypes = [vp, i, i, i, ul]
        t.XTestFakeButtonEvent.argtypes = [vp, ui, i, ul]
        t.XTestFakeKeyEvent.argtypes = [vp, ui, i, ul]

    def _has_xtest(self):
        a, b, c, d = (ctypes.c_int() for _ in range(4))
        refs = [ctypes.byref(v) for v in (a, b, c, d)]
        return bool(self.xt.XTestQueryExtension(self.display, *refs))

    def flush(self):
        self.x11.XSync(self.display, 0)

    def move(self, x, y):
        self.xt.XTestFakeMotionEvent(self.display, -1, int(x), int(y), 0)
        self.flush()

    def button(self, number, down):
        self.xt.XTestFakeButtonEvent(self.display, number, 1 if down else 0, 0)
        (self.buttons.add if down else self.buttons.discard)(number)
        self.flush()

    def key(self, name, down):
        code = self.keycode(name)
        self.xt.XTestFakeKeyEvent(self.display, code, 1 if down else 0, 0)
        (self.keys.add if down else self.keys.discard)(name)
        self.flush()

    def keycode(self, name):
        code = self.x11.XKeysymToKeycode(self.display, KEYSYMS[name])
        if code == 0:
            raise DriveAbort(f"no keycode for {name} on this keyboard map")
        return code

    def pointer_chain(self):
        """The windows under the pointer from the top-level down, and the pointer's root x, y."""
        chain, window = [], self.root
        rx = ry = 0
        for _ in range(16):
            root, child = ctypes.c_ulong(), ctypes.c_ulong()
            vals = [ctypes.c_int() for _ in range(4)]
            mask = ctypes.c_uint()
            self.x11.XQueryPointer(self.display, window, ctypes.byref(root), ctypes.byref(child),
                                   *[ctypes.byref(v) for v in vals], ctypes.byref(mask))
            rx, ry = vals[0].value, vals[1].value
            if not child.value:
                break
            chain.append(child.value)
            window = child.value
        return chain, (rx, ry)

    def raise_window(self, window):
        self.x11.XRaiseWindow(self.display, window)
        self.flush()

    def focus(self):
        """The window holding the X keyboard focus."""
        window, revert = ctypes.c_ulong(), ctypes.c_int()
        self.x11.XGetInputFocus(self.display, ctypes.byref(window), ctypes.byref(revert))
        return window.value

    def type_key(self, window, name, shift=False):
        """Presses and releases one key (with Shift around it) into `window`.

        Under WSLg the window manager gives an X window the keyboard focus only while its Windows
        twin is the active Windows window, and takes back any focus a client sets a moment later.
        So the focus is set to `window` in the same request batch as the key events: the server
        delivers them to it before the window manager reverts the focus.
        """
        names = (["Shift_L"] if shift else []) + [name]
        codes = [self.keycode(n) for n in names]
        self.x11.XSetInputFocus(self.display, window, 0, 0)  # RevertToNone, CurrentTime
        for n, code in zip(names, codes):
            self.keys.add(n)
            self.xt.XTestFakeKeyEvent(self.display, code, 1, 0)
        for n, code in reversed(list(zip(names, codes))):
            self.xt.XTestFakeKeyEvent(self.display, code, 0, 0)
            self.keys.discard(n)
        self.flush()

    def release_all(self):
        """Lets go of every button and key this run pressed, Shift first."""
        for name in sorted(self.keys, key=lambda k: k != "Shift_L"):
            try:
                self.key(name, False)
            except DriveAbort:
                pass
        for number in list(self.buttons):
            self.button(number, False)
        self.flush()


# ---------------------------------------------------------------------------------------------
# The app: launched by pid, found on screen, read through its drive files.


class App:
    def __init__(self, binary, args, workdir):
        self.out = os.path.join(workdir, "drive.json")
        self.state_path = self.out + ".state"
        env = dict(os.environ, WAYLAND_DISPLAY="")
        env.pop("OPENMIXER_THEME", None)
        env["OPENMIXER_PREFS"] = os.path.join(workdir, "preferences.json")
        self.log = open(os.path.join(workdir, "app.log"), "wb")
        cmd = [binary, "--ui=tauri", "--mock-device", f"--ui-drive-out={self.out}", *args]
        self.proc = subprocess.Popen(cmd, env=env, stdout=self.log, stderr=subprocess.STDOUT)
        self.window = None
        self.origin = (0, 0)
        self.layout = None

    def alive(self):
        return self.proc.poll() is None

    def stop(self):
        if self.alive():
            self.proc.terminate()
            try:
                self.proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait(timeout=10)
        self.log.close()

    def wait_ready(self, ready):
        """Waits for the layout map, the state file, the window, and `ready(state)`."""
        deadline = time.monotonic() + START_TIMEOUT_S
        while time.monotonic() < deadline:
            if not self.alive():
                raise DriveAbort(f"the app exited with code {self.proc.returncode} during start")
            if self._loaded() and ready(self.state()):
                self.find_window()
                return
            time.sleep(0.2)
        raise DriveAbort("the app did not get ready in time (layout, state or decks)")

    def _loaded(self):
        try:
            with open(self.out, encoding="utf-8") as f:
                self.layout = json.load(f)
            return self.state() is not None
        except (OSError, ValueError):
            return False

    def state(self):
        for _ in range(5):
            try:
                with open(self.state_path, encoding="utf-8") as f:
                    return json.load(f)
            except (OSError, ValueError):
                time.sleep(0.02)
        return None

    def find_window(self):
        tree = subprocess.run(["xwininfo", "-root", "-tree"], capture_output=True, text=True,
                              check=True).stdout
        found = []
        for line in tree.splitlines():
            # "OpenMixer DJ" since the rename; plain "OpenMixer" still matches an older build.
            m = re.match(r'\s*(0x[0-9a-f]+) "OpenMixer(?: DJ)?":.*?(\d+)x(\d+)[+-]\d+[+-]\d+'
                         r'\s+\+(-?\d+)\+(-?\d+)', line)
            if m:
                found.append((int(m.group(1), 16), int(m.group(4)), int(m.group(5))))
        view = self.layout["viewport"]
        dpr = self.layout["devicePixelRatio"]
        mine = [w for w in found if self._is_mine(w[0])]
        if len(mine) != 1:
            raise DriveAbort(f"expected one OpenMixer DJ window of pid {self.proc.pid}, found {len(mine)}")
        self.window, x, y = mine[0]
        self.origin = (x, y)
        self.scale = dpr
        self.viewport = (view["width"], view["height"])

    def _is_mine(self, window):
        out = subprocess.run(["xprop", "-id", hex(window), "_NET_WM_PID"], capture_output=True,
                             text=True).stdout
        m = re.search(r"= (\d+)", out)
        return m is not None and int(m.group(1)) == self.proc.pid

    def rect(self, key):
        rect = self.layout["controls"].get(key)
        if rect is None:
            raise KeyError(f"no control {key} in the layout map")
        return rect

    def spec(self, key):
        return self.layout["specs"][key]

    def screen(self, x_css, y_css):
        return (round(self.origin[0] + x_css * self.scale), round(self.origin[1] + y_css * self.scale))


# ---------------------------------------------------------------------------------------------
# Control laws, as web/src/lib/controls/range.ts maps them.


class Law:
    def __init__(self, spec):
        self.min, self.max, self.default = spec["min"], spec["max"], spec["default"]
        detent = spec["detent"]
        self.detent = detent if detent is not None and self.min < detent < self.max else None
        law = spec["law"]
        self.knots = law["knots"] if law["kind"] == "knots" else None
        self.snap = spec["snap"]

    def value(self, pos):
        f = min(max(pos, 0.0), 1.0)
        if self.knots:
            return _knot(self.knots, f, 0, 1)
        d = self.detent
        if d is None:
            return self.min + (self.max - self.min) * f
        return self.min + (d - self.min) * f / 0.5 if f <= 0.5 else d + (self.max - d) * (f - 0.5) / 0.5

    def position(self, value):
        if self.knots:
            return min(max(_knot(self.knots, value, 1, 0), 0.0), 1.0)
        v = min(max(value, self.min), self.max)
        d = self.detent
        if d is None:
            return (v - self.min) / (self.max - self.min)
        return 0.5 * (v - self.min) / (d - self.min) if v <= d else 0.5 + 0.5 * (v - d) / (self.max - d)


def _knot(knots, x, xi, yi):
    for n, k in enumerate(knots):
        if k[xi] >= x:
            if n == 0 or k[xi] == x:
                return k[yi]
            a = knots[n - 1]
            return a[yi] + (k[yi] - a[yi]) * (x - a[xi]) / (k[xi] - a[xi])
    return knots[-1][yi]


def next_mark(position, size, direction):
    """keys.ts nextMark: the next multiple of `size` in `direction`."""
    at = position / size
    mark = (int(_floor(at + 1e-6)) + 1) if direction > 0 else (int(_ceil(at - 1e-6)) - 1)
    return min(max(mark * size, 0.0), 1.0)


def _floor(v):
    return int(v // 1)


def _ceil(v):
    return -int((-v) // 1)


# ---------------------------------------------------------------------------------------------
# The driver: presses, drags and keys at controls, and the check log.


class Driver:
    def __init__(self, xin, app, results, verbose=False):
        self.x, self.app, self.results = xin, app, results
        self.verbose = verbose
        # Keys sent a second time (see `key`); the summary reports them.
        self.resent = 0

    # -- reading

    def state(self):
        s = self.app.state()
        if s is None:
            raise DriveAbort("the state file cannot be read")
        return s

    def settle(self, seconds=STATE_PERIOD_S * 3):
        time.sleep(seconds)
        return self.state()

    def wait(self, predicate, timeout=2.0):
        """The first state for which `predicate` holds, or the last one read after `timeout`."""
        deadline = time.monotonic() + timeout
        s = self.state()
        while not predicate(s) and time.monotonic() < deadline:
            time.sleep(0.05)
            s = self.state()
        return s

    def deck(self, d, s=None):
        return (s or self.state())["decks"][d]

    def value(self, key, s=None):
        """The engine value a continuous control's key writes, from a state."""
        s = s or self.state()
        m = re.match(r"ch(\d+)\.(.+)", key)
        if m:
            ch = s["channels"][int(m.group(1))]
            part = m.group(2)
            if part.startswith("eq."):
                return ch["eq"][["low", "mid", "high"].index(part[3:])]
            return ch[part]
        if key == "mixer.crossfader":
            return s["crossfader"]
        if key == "mixer.master":
            return s["master_gain"]
        m = re.match(r"deck(\d+)\.tempo", key)
        if m:
            return s["decks"][int(m.group(1))]["rate"]
        raise KeyError(key)

    # -- pointer

    def centre(self, key):
        r = self.app.rect(key)
        return r["x"] + r["width"] / 2, r["y"] + r["height"] / 2

    def point(self, x_css, y_css):
        """Moves the pointer to a CSS point and checks the app's window is under it."""
        sx, sy = self.app.screen(x_css, y_css)
        for _ in range(3):
            self.x.move(sx, sy)
            time.sleep(0.01)
            chain, at = self.x.pointer_chain()
            if self.app.window not in chain:
                raise DriveAbort(f"the pointer at {at} is not over the OpenMixer DJ window "
                                 f"({[hex(w) for w in chain]})")
            if at == (sx, sy):
                return
        raise Interrupted(f"the pointer keeps moving away from {(sx, sy)} (at {at})")

    def glide(self, x_css, y_css, step_px=MOVE_STEP_PX, pause=0.008):
        """Moves the pointer from where it is to a CSS point in small steps (a drag's moves)."""
        chain, (rx, ry) = self.x.pointer_chain()
        tx, ty = self.app.screen(x_css, y_css)
        n = max(1, int(max(abs(tx - rx), abs(ty - ry)) / step_px))
        for i in range(1, n + 1):
            self.x.move(rx + (tx - rx) * i / n, ry + (ty - ry) * i / n)
            time.sleep(pause)
        chain, at = self.x.pointer_chain()
        if at != (tx, ty) or self.app.window not in chain:
            raise Interrupted(f"the pointer was moved by something else: at {at}, sent to {(tx, ty)}, "
                             f"over {[hex(w) for w in chain]} (app {hex(self.app.window)}: "
                             f"{describe(chain[0] if chain else 0)})")

    def press(self, shift=False):
        if shift:
            self.x.key("Shift_L", True)
            time.sleep(0.03)
        self.x.button(1, True)
        time.sleep(0.03)

    def release(self, shift=False):
        self.x.button(1, False)
        time.sleep(0.03)
        if shift:
            self.x.key("Shift_L", False)

    def click_at(self, x_css, y_css, shift=False):
        self.point(x_css, y_css)
        self.press(shift)
        self.release(shift)

    def click(self, key, fx=0.5, fy=0.5, shift=False):
        r = self.app.rect(key)
        self.click_at(r["x"] + r["width"] * fx, r["y"] + r["height"] * fy, shift)

    def double_click_at(self, x_css, y_css):
        self.point(x_css, y_css)
        for _ in range(2):
            self.x.button(1, True)
            time.sleep(0.03)
            self.x.button(1, False)
            time.sleep(0.06)

    def wheel(self, notches, x_css, y_css):
        """Positive notches scroll up (X button 4), negative down (button 5)."""
        self.point(x_css, y_css)
        number = 4 if notches > 0 else 5
        for _ in range(abs(notches)):
            self.x.button(number, True)
            self.x.button(number, False)
            time.sleep(0.12)

    # -- keys

    def tap(self, name, shift=False):
        self.x.type_key(self.app.window, name, shift)
        time.sleep(0.08)

    def key(self, name, done, read, shift=False, timeout=2.0):
        """Taps a key and waits for `done(state)`. A key that changed nothing `read` sees is sent
        once more: WSLg takes the focus back right after `type_key`, and a button's Space acts on
        its release, which can land after the page has lost the focus."""
        before = read(self.state())
        self.tap(name, shift)
        s = self.wait(done, timeout)
        if done(s) or read(s) != before:
            return s
        self.resent += 1
        self.log(f"key {name} changed nothing; sent again")
        self.tap(name, shift)
        return self.wait(done, timeout)

    # -- checks

    def log(self, text):
        if self.verbose:
            print(f"      {text}", flush=True)

    def check(self, name, ok, expected, actual):
        self.results.record(name, ok, expected, actual)


class Results:
    def __init__(self):
        self.rows = []
        # Key presses sent a second time over the run (`Driver.key`).
        self.resent = 0

    def record(self, name, ok, expected, actual):
        self.rows.append((name, ok))
        print(f"{'PASS' if ok else 'FAIL'}  {name}  {expected}  {actual}", flush=True)

    def failed(self):
        return [n for n, ok in self.rows if not ok]


def describe(window):
    """A window's type and size, for a message about what is under the pointer."""
    props = subprocess.run(["xprop", "-id", hex(window), "_NET_WM_WINDOW_TYPE", "WM_CLASS"],
                           capture_output=True, text=True).stdout
    info = subprocess.run(["xwininfo", "-id", hex(window)], capture_output=True, text=True).stdout
    size = re.search(r"-geometry (\S+)", info)
    override = re.search(r"Override Redirect State: (\w+)", info)
    return " ".join(props.split()) + f" geometry={size.group(1) if size else '?'}" + \
        f" override={override.group(1) if override else '?'}"


def near(a, b, tol):
    return a is not None and b is not None and abs(a - b) <= tol


def fmt(v):
    if isinstance(v, float):
        return f"{v:.5g}"
    return str(v)


# ---------------------------------------------------------------------------------------------
# The checks.


def buttons(dr, d):
    play, sync, cue = f"deck{d}.play", f"deck{d}.sync", f"deck{d}.cue"
    for key, flag, want, label in ((play, "playing", True, "click starts"),
                                   (play, "playing", False, "click again stops"),
                                   (sync, "sync", True, "click on"),
                                   (sync, "sync", False, "click off")):
        dr.click(key)
        s = dr.wait(lambda s: s["decks"][d][flag] == want)
        got = dr.deck(d, s)[flag]
        dr.check(f"{key} {label}", got == want, f"{flag}={want}", f"{flag}={got}")
    dr.click(play)
    dr.wait(lambda s: s["decks"][d]["playing"])
    time.sleep(0.5)
    dr.click(cue)
    rate = dr.deck(d)["file_sample_rate"]
    s = dr.wait(lambda s: not s["decks"][d]["playing"] and s["decks"][d]["position_frames"] <= 0.01 * rate)
    deck = dr.deck(d, s)
    ok = not deck["playing"] and deck["position_frames"] <= 0.01 * rate
    dr.check(f"{cue} on a playing deck stops at the start", ok, "playing=false position<=10ms",
             f"playing={deck['playing']} position={deck['position_frames']}")


def knob_drag(dr, key, dy, shift=False):
    """Presses a knob's centre, drags `dy` CSS px (negative is up) in steps, releases."""
    x, y = dr.centre(key)
    dr.point(x, y)
    dr.press(shift)
    dr.glide(x, y + dy)
    dr.release(shift)


def knob_position(dr, key, law, expect_pos=None):
    def settled(s):
        return expect_pos is None or near(law.position(dr.value(key, s)), expect_pos, 0.002)
    s = dr.wait(settled, timeout=1.5)
    return law.position(dr.value(key, s)), dr.value(key, s)


def knobs(dr, key):
    law = Law(dr.app.spec(key))
    x, y = dr.centre(key)
    dr.double_click_at(x, y)
    pos0, v = knob_position(dr, key, law, law.position(law.default))
    dr.check(f"{key} double-click restores the default", near(v, law.default, 1e-4 * (law.max - law.min) + 1e-6),
             fmt(law.default), fmt(v))
    # Down to a quarter of the travel, clear of both ends and the detent.
    knob_drag(dr, key, (pos0 - 0.25) * KNOB_TRAVEL_PX)
    start, _ = knob_position(dr, key, law, 0.25)
    knob_drag(dr, key, -80)
    up, v_up = knob_position(dr, key, law, start + 0.5)
    dr.check(f"{key} drag up 80 px turns half the travel", near(up - start, 0.5, 0.05),
             "+0.500 travel (+-0.05)", f"{up - start:+.3f} travel (value {fmt(v_up)})")
    knob_drag(dr, key, 80)
    down, v_down = knob_position(dr, key, law, up - 0.5)
    dr.check(f"{key} drag down 80 px turns back", near(down - up, -0.5, 0.05),
             "-0.500 travel (+-0.05)", f"{down - up:+.3f} travel (value {fmt(v_down)})")
    knob_drag(dr, key, -80, shift=True)
    fine, v_fine = knob_position(dr, key, law, down + 0.05)
    dr.log(f"{key} fine drag from {down:.4f} to {fine:.4f}")
    dr.check(f"{key} Shift drag up 80 px turns a tenth as far", near(fine - down, 0.05, 0.01),
             "+0.050 travel (+-0.01)", f"{fine - down:+.3f} travel (value {fmt(v_fine)})")
    if law.snap:
        detent_pull(dr, key, law)
    dr.double_click_at(x, y)
    knob_position(dr, key, law, law.position(law.default))


def detent_pull(dr, key, law):
    """Drags slowly across the detent, reading the engine at every step, with and without Shift."""
    at, width = law.snap["at"], law.snap["width"]
    x, y = dr.centre(key)
    for shift in (False, True):
        dr.double_click_at(x, y)
        knob_position(dr, key, law, law.position(at))
        # Odd offsets: no step lands on the detent's own position, so a reading of it is the pull.
        # Without Shift it starts below the widest pull (EQ above 0 dB: 22 px) and crosses it; with
        # Shift it stays inside the narrowest (the filter's 1.5% of travel is 24 px fine).
        reach = 31 if not shift else 9
        dr.point(x, y)
        dr.press(shift)
        dr.glide(x, y + reach)
        seen = []
        for step in range(0, 2 * reach + 1, 2):
            dr.glide(x, y + reach - step)
            time.sleep(STATE_PERIOD_S * 1.3)
            seen.append(dr.value(key))
        dr.release(shift)
        dr.log(f"{key} detent drag shift={shift}: {', '.join(fmt(v) for v in seen)}")
        inside = [v for v in seen if abs(v - at) <= width + 1e-6]
        held = [v for v in inside if abs(v - at) <= 1e-6]
        loose = [v for v in inside if abs(v - at) > 1e-6]
        if not shift:
            dr.check(f"{key} slow drag across the detent snaps", bool(held) and not loose,
                     f"inside +-{fmt(width)}: only {fmt(at)}",
                     f"{len(held)} at the detent, {len(loose)} off it ({', '.join(fmt(v) for v in loose[:3])})")
        else:
            dr.check(f"{key} Shift drag across the detent does not snap", bool(loose),
                     f"values inside +-{fmt(width)} other than {fmt(at)}",
                     f"{len(loose)} off the detent, {len(held)} on it")


def fader_point(dr, key, pos):
    """The CSS point of travel `pos` on a fader: the thumb's centre runs half a thumb in from each end."""
    r, t = dr.app.rect(key), dr.app.rect(f"{key}.thumb")
    if r["width"] > r["height"]:
        half = t["width"] / 2
        return r["x"] + half + pos * (r["width"] - 2 * half), r["y"] + r["height"] / 2
    half = t["height"] / 2
    return r["x"] + r["width"] / 2, r["y"] + half + (1 - pos) * (r["height"] - 2 * half)


def fader_tolerance(dr, key):
    """One and a half CSS px of travel: the click lands on a whole device pixel."""
    r, t = dr.app.rect(key), dr.app.rect(f"{key}.thumb")
    length = r["width"] - t["width"] if r["width"] > r["height"] else r["height"] - t["height"]
    return 1.5 / length


def fader_pos(dr, key, law, expect=None, tol=0.0):
    s = dr.wait(lambda s: expect is None or near(law.position(dr.value(key, s)), expect, tol), timeout=1.5)
    v = dr.value(key, s)
    return law.position(v), v


def faders(dr, key):
    law, tol = Law(dr.app.spec(key)), fader_tolerance(dr, key)
    for pos in (0.25, 0.5, 1.0):
        dr.click_at(*fader_point(dr, key, pos))
        got, v = fader_pos(dr, key, law, pos, tol)
        dr.check(f"{key} click at {pos:.0%} of the track", near(got, pos, tol),
                 f"{fmt(law.value(pos))} (law at {pos:.2f})", f"{fmt(v)} (travel {got:.4f})")
    fader_drag(dr, key, law, tol)
    dr.double_click_at(*fader_point(dr, key, law.position(dr.value(key))))
    s = dr.wait(lambda s: near(dr.value(key, s), law.default, 1e-4))
    dr.check(f"{key} double-click restores the default", near(dr.value(key, s), law.default, 1e-4),
             fmt(law.default), fmt(dr.value(key, s)))


def fader_drag(dr, key, law, tol):
    start = law.position(dr.value(key))
    dr.point(*fader_point(dr, key, start))
    dr.press()
    dr.glide(*fader_point(dr, key, 0.6))
    time.sleep(0.3)
    mid, _ = fader_pos(dr, key, law, 0.6, tol)
    dr.glide(*fader_point(dr, key, 0.3))
    dr.release()
    end, v = fader_pos(dr, key, law, 0.3, tol)
    dr.check(f"{key} drag follows the thumb", near(mid, 0.6, tol) and near(end, 0.3, tol),
             "travel 0.6 mid-drag, 0.3 at release", f"{mid:.4f} mid-drag, {end:.4f} at release")
    samples = []
    for _ in range(6):
        time.sleep(0.1)
        samples.append(dr.value(key))
    stable = max(samples) - min(samples) <= 1e-6 and near(law.position(samples[-1]), 0.3, tol)
    dr.check(f"{key} stays where it was dropped for 500 ms", stable, f"{fmt(v)} unchanged",
             ", ".join(fmt(s) for s in samples))


def tempo(dr, d):
    key = f"deck{d}.tempo"
    law = Law(dr.app.spec(key))
    before = dr.value(key)
    start = law.position(before)
    dr.point(*fader_point(dr, key, start))
    dr.press()
    dr.glide(*fader_point(dr, key, start + 0.25))
    dr.release()
    s = dr.wait(lambda s: dr.value(key, s) > before + 0.01)
    rate = dr.value(key, s)
    ok = rate > before and law.min <= rate <= law.max
    dr.check(f"{key} drag up raises the rate inside its range", ok,
             f"> {fmt(before)} within [{fmt(law.min)}, {fmt(law.max)}]", fmt(rate))
    dr.double_click_at(*fader_point(dr, key, law.position(rate)))
    s = dr.settle(0.5)
    after = dr.value(key, s)
    tol = (law.max - law.min) * fader_tolerance(dr, key)
    dr.check(f"{key} double-click does not reset", near(after, rate, tol) and not near(after, law.default, 1e-4),
             f"{fmt(rate)} (not {fmt(law.default)})", fmt(after))
    dr.click_at(*fader_point(dr, key, 0.5))
    dr.wait(lambda s: near(dr.value(key, s), law.default, tol))


def seek_overview(dr, d, frac):
    dr.click(f"deck{d}.overview", fx=frac)
    length = dr.deck(d)["length_frames"]
    return dr.wait(lambda s: near(s["decks"][d]["position_frames"], frac * length, 0.01 * length))


def hotcues(dr, d):
    pad = f"deck{d}.hotcue2"
    rate = dr.deck(d)["file_sample_rate"]
    seek_overview(dr, d, 0.3)
    at = dr.settle()["decks"][d]["position_frames"]
    dr.click(pad)
    s = dr.wait(lambda s: s["decks"][d]["hotcues"][2] is not None)
    cue = dr.deck(d, s)["hotcues"][2]
    dr.check(f"{pad} click on an empty pad sets cue 3 at the playhead", near(cue, at, 0.1 * rate),
             f"{at} (+-0.1 s)", fmt(cue))
    seek_overview(dr, d, 0.7)
    dr.click(pad)
    s = dr.wait(lambda s: cue is not None and near(s["decks"][d]["position_frames"], cue, 0.02 * rate))
    pos = dr.deck(d, s)["position_frames"]
    dr.check(f"{pad} click on a set pad jumps to it", near(pos, cue, 0.02 * rate), f"{cue} (+-20 ms)", pos)
    dr.click(pad, shift=True)
    s = dr.wait(lambda s: s["decks"][d]["hotcues"][2] is None)
    dr.check(f"{pad} Shift-click clears it", dr.deck(d, s)["hotcues"][2] is None, "None",
             dr.deck(d, s)["hotcues"][2])


def loops(dr, d):
    key, play = f"deck{d}.loop.4", f"deck{d}.play"
    deck = dr.deck(d)
    if deck["bpm"] is None:
        dr.check(f"{key} needs a beat grid", False, "bpm", "None")
        return
    dr.click(play)
    dr.wait(lambda s: s["decks"][d]["playing"])
    dr.click(key)
    s = dr.wait(lambda s: s["decks"][d]["looping"])
    deck = dr.deck(d, s)
    beats4 = 4 * 60.0 / deck["bpm"] * deck["file_sample_rate"]
    span = deck["loop_end"] - deck["loop_start"]
    ok = deck["looping"] and near(span, beats4, 0.01 * beats4)
    dr.check(f"{key} click while playing loops four beats", ok, f"looping, {beats4:.0f} frames (+-1%)",
             f"looping={deck['looping']}, {span} frames")
    dr.click(key)
    s = dr.wait(lambda s: not s["decks"][d]["looping"])
    dr.check(f"{key} click on the lit size ends the loop", not dr.deck(d, s)["looping"], "looping=false",
             f"looping={dr.deck(d, s)['looping']}")
    dr.click(play)
    dr.wait(lambda s: not s["decks"][d]["playing"])


def overview(dr, d):
    key = f"deck{d}.overview"
    length = dr.deck(d)["length_frames"]
    s = seek_overview(dr, d, 0.5)
    pos = dr.deck(d, s)["position_frames"]
    dr.check(f"{key} click at 50% seeks to half the track", near(pos, length / 2, 0.01 * length),
             f"{length // 2} (+-1%)", pos)
    r = dr.app.rect(key)
    y = r["y"] + r["height"] / 2
    dr.point(r["x"] + 0.2 * r["width"], y)
    dr.press()
    dr.glide(r["x"] + 0.6 * r["width"], y)
    time.sleep(0.3)
    mid = dr.deck(d)["position_frames"]
    dr.glide(r["x"] + 0.8 * r["width"], y)
    dr.release()
    s = dr.wait(lambda s: near(s["decks"][d]["position_frames"], 0.8 * length, 0.01 * length))
    end = dr.deck(d, s)["position_frames"]
    ok = near(mid, 0.6 * length, 0.01 * length) and near(end, 0.8 * length, 0.01 * length)
    dr.check(f"{key} drag follows", ok, f"{0.6 * length:.0f} mid-drag, {0.8 * length:.0f} at release",
             f"{mid}, {end}")


def scrub(dr, d, px=200):
    """Drags lane `d` `px` CSS px left from its centre; the frames the position moved."""
    lane = dr.app.rect(f"deck{d}.lane")
    x, y = lane["x"] + lane["width"] / 2, lane["y"] + lane["height"] / 2
    before = dr.settle()["decks"][d]["position_frames"]
    dr.point(x, y)
    dr.press()
    dr.glide(x - px, y)
    dr.release()
    s = dr.settle(0.5)
    return before, dr.deck(d, s)["position_frames"]


def lane_seconds_per_px(dr, d):
    lane = dr.app.rect(f"deck{d}.lane")
    return DEFAULT_VISIBLE_SECONDS / lane["width"]


def lanes(dr, d):
    key = f"deck{d}.lane"
    seek_overview(dr, d, 0.5)
    rate = dr.deck(d)["file_sample_rate"]
    expected = 200 * lane_seconds_per_px(dr, d) * rate
    before, after = scrub(dr, d)
    dr.check(f"{key} drag 200 px left moves 200 px of time forward", near(after - before, expected, 0.05 * expected),
             f"+{expected:.0f} frames (+-5%)", f"{after - before:+d}")
    samples = []
    for _ in range(5):
        time.sleep(0.2)
        samples.append(dr.deck(d)["position_frames"])
    dr.check(f"{key} does not snap back within 1 s", all(s == after for s in samples), f"{after} held",
             ", ".join(str(s) for s in samples))


def wheel_zoom(dr, d=0):
    key = f"deck{d}.lane"
    lane = dr.app.rect(key)
    centre = (lane["x"] + lane["width"] / 2, lane["y"] + lane["height"] / 2)
    seek_overview(dr, d, 0.5)
    b0, a0 = scrub(dr, d)
    base = a0 - b0
    dr.wheel(3, *centre)
    b1, a1 = scrub(dr, d)
    zoomed = a1 - b1
    expected = base / ZOOM_STEP ** 3
    dr.check(f"{key} wheel up 3 notches zooms in 1.15^3", near(zoomed, expected, 0.05 * expected),
             f"+{expected:.0f} frames per 200 px (+-5%)", f"{zoomed:+d}")
    dr.wheel(-3, *centre)
    b2, a2 = scrub(dr, d)
    dr.check(f"{key} wheel down 3 notches restores the zoom", near(a2 - b2, base, 0.05 * base),
             f"+{base} frames per 200 px (+-5%)", f"{a2 - b2:+d}")


def keys_knob(dr, key="ch0.eq.high"):
    law = Law(dr.app.spec(key))
    dr.click(key)
    time.sleep(0.2)
    steps = [("Up", False, lambda p: law.value(next_mark(p, STEP, 1)), "Arrow Up"),
             ("Up", True, lambda p: law.value(next_mark(p, BIG_STEP, 1)), "Shift+Arrow Up"),
             ("Down", False, lambda p: law.value(next_mark(p, STEP, -1)), "Arrow Down"),
             ("Home", False, lambda p: law.min, "Home"),
             ("End", False, lambda p: law.max, "End"),
             ("Delete", False, lambda p: law.default, "Delete")]
    slider_keys(dr, key, law, steps)


def slider_keys(dr, key, law, steps):
    for name, shift, expect, label in steps:
        now = dr.settle()
        target = expect(law.position(dr.value(key, now)))
        tol = 1e-4 * (law.max - law.min) + 1e-5
        s = dr.key(name, lambda s: near(dr.value(key, s), target, tol), lambda s: dr.value(key, s), shift)
        dr.check(f"{key} key {label}", near(dr.value(key, s), target, tol), fmt(target), fmt(dr.value(key, s)))


def keys_fader(dr, key="ch1.fader"):
    law = Law(dr.app.spec(key))
    dr.click_at(*fader_point(dr, key, law.position(dr.value(key))))
    time.sleep(0.2)
    steps = [("Down", False, lambda p: law.value(next_mark(p, STEP, -1)), "Arrow Down"),
             ("Down", True, lambda p: law.value(next_mark(p, BIG_STEP, -1)), "Shift+Arrow Down"),
             ("Up", False, lambda p: law.value(next_mark(p, STEP, 1)), "Arrow Up"),
             ("Home", False, lambda p: law.min, "Home"),
             ("End", False, lambda p: law.max, "End"),
             ("Delete", False, lambda p: law.default, "Delete")]
    slider_keys(dr, key, law, steps)


def focus_lane(dr, d):
    """Clicks the deck's overview (its seek is harmless here) and tabs to the lane's slider after it."""
    seek_overview(dr, d, 0.5)
    dr.tap("Tab")
    time.sleep(0.1)


def keys_lane(dr, d=0):
    key = f"deck{d}.lane"
    deck = dr.deck(d)
    beat = 60.0 / deck["bpm"] * deck["file_sample_rate"] if deck["bpm"] else 0.5 * deck["file_sample_rate"]
    length = deck["length_frames"]
    focus_lane(dr, d)
    for name, shift, delta, label in (("Right", False, beat, "Arrow Right (one beat)"),
                                      ("Left", False, -beat, "Arrow Left (one beat)"),
                                      ("Right", True, LONG_STEP_BEATS * beat, "Shift+Arrow Right (four beats)")):
        before = dr.settle()["decks"][d]["position_frames"]
        s = dr.key(name, lambda s: near(s["decks"][d]["position_frames"], before + delta, 0.01 * beat + 2),
                   lambda s: s["decks"][d]["position_frames"], shift)
        pos = dr.deck(d, s)["position_frames"]
        dr.check(f"{key} key {label}", near(pos, before + delta, 0.01 * beat + 2), f"{before + delta:.0f}", pos)
    for name, target, label in (("Home", 0, "Home"), ("End", length - 1, "End")):
        s = dr.key(name, lambda s: near(s["decks"][d]["position_frames"], target, 2),
                   lambda s: s["decks"][d]["position_frames"])
        pos = dr.deck(d, s)["position_frames"]
        dr.check(f"{key} key {label}", near(pos, target, 2), target, pos)
    keys_zoom(dr, d)


def keys_zoom(dr, d):
    key = f"deck{d}.lane"
    seek_overview(dr, d, 0.5)
    b, a = scrub(dr, d)
    base = a - b
    for taps, factor, label in ((["KP_Add"], 1 / ZOOM_STEP, "+ zooms in one notch"),
                                (["minus"], ZOOM_STEP, "- zooms out one notch"),
                                (["KP_Add", "KP_Add", "0"], 1.0, "0 resets the zoom")):
        focus_lane(dr, d)
        for name in taps:
            dr.tap(name)
        b, a = scrub(dr, d)
        # `-` after `+` brings the view back to 8 s; each row starts from the zoom the last left.
        expected = base * (factor if label.startswith("+") else 1.0)
        dr.check(f"{key} key {label}", near(a - b, expected, 0.05 * expected), f"+{expected:.0f} frames per 200 px",
                 f"{a - b:+d}")


def keys_space(dr, d=0):
    play, loop = f"deck{d}.play", f"deck{d}.loop.4"
    dr.click(play)
    dr.wait(lambda s: s["decks"][d]["playing"])
    s = dr.key("space", lambda s: not s["decks"][d]["playing"], lambda s: s["decks"][d]["playing"])
    dr.check(f"{play} key Space toggles it", not dr.deck(d, s)["playing"], "playing=false",
             f"playing={dr.deck(d, s)['playing']}")
    dr.click(play)
    dr.wait(lambda s: s["decks"][d]["playing"])
    dr.click(loop)
    dr.wait(lambda s: s["decks"][d]["looping"])
    s = dr.key("space", lambda s: not s["decks"][d]["looping"], lambda s: s["decks"][d]["looping"])
    dr.check(f"{loop} key Space ends the lit loop", not dr.deck(d, s)["looping"], "looping=false",
             f"looping={dr.deck(d, s)['looping']}")
    dr.click(play)
    dr.wait(lambda s: not s["decks"][d]["playing"])


def keys_pad(dr, d=1):
    pad = f"deck{d}.hotcue5"
    seek_overview(dr, d, 0.4)
    dr.click(pad)
    dr.wait(lambda s: s["decks"][d]["hotcues"][5] is not None)
    s = dr.key("Delete", lambda s: s["decks"][d]["hotcues"][5] is None, lambda s: s["decks"][d]["hotcues"][5])
    dr.check(f"{pad} key Delete clears it", dr.deck(d, s)["hotcues"][5] is None, "None", dr.deck(d, s)["hotcues"][5])


def keyboard(dr):
    keys_knob(dr)
    keys_fader(dr)
    keys_lane(dr, 0)
    keys_space(dr, 0)
    keys_pad(dr, 1)


def disabled(dr):
    before = dr.settle()
    for key in ("deck0.play", "deck0.cue", "deck0.hotcue2", "deck0.loop.4", "deck1.play", "deck1.loop.4"):
        dr.click(key)
    after = dr.settle(0.6)
    alive = dr.app.alive()

    def deck_view(s):
        return [(d["playing"], d["looping"], d["position_frames"], d["hotcues"]) for d in s["decks"]]
    same = deck_view(before) == deck_view(after)
    dr.check("empty decks: Play, Cue, pad and loop clicks change nothing", same and alive,
             "state unchanged, app running", f"unchanged={same} running={alive}")


# ---------------------------------------------------------------------------------------------
# Runs.


def both_loaded(s):
    return s is not None and all(d["length_frames"] > 0 and d["bpm"] for d in s["decks"][:2])


def any_state(s):
    return s is not None


def suite_loaded(dr, only):
    groups = [
        ("buttons", lambda: [buttons(dr, d) for d in (0, 1)]),
        ("knobs", lambda: [knobs(dr, k) for k in (
            "ch0.trim", "ch0.eq.high", "ch0.eq.mid", "ch0.eq.low", "ch0.filter",
            "ch1.trim", "ch1.eq.high", "ch1.eq.mid", "ch1.eq.low", "ch1.filter", "mixer.master")]),
        ("faders", lambda: [faders(dr, k) for k in ("ch0.fader", "ch1.fader", "mixer.crossfader")]),
        ("tempo", lambda: [tempo(dr, d) for d in (0, 1)]),
        ("hotcues", lambda: [hotcues(dr, d) for d in (0, 1)]),
        ("loops", lambda: [loops(dr, d) for d in (0, 1)]),
        ("overview", lambda: [overview(dr, d) for d in (0, 1)]),
        ("lanes", lambda: [lanes(dr, d) for d in (0, 1)]),
        ("wheel", lambda: wheel_zoom(dr, 0)),
        ("keys", lambda: keyboard(dr)),
    ]
    for name, run in groups:
        if only and not name.startswith(only):
            continue
        try:
            run()
        except Interrupted as error:
            dr.x.release_all()
            dr.check(f"{name} group ran undisturbed", False, "no other pointer input", str(error))
        except (DriveAbort, KeyboardInterrupt, SystemExit):
            raise
        except Exception as error:  # one broken group must not hide the others' results
            dr.check(f"{name} group ran", False, "no error", f"{type(error).__name__}: {error}")


def session(binary, args, ready, body, results, verbose=False):
    """Starts the app, drives `body`, and always stops it and lets go of every input."""
    workdir = tempfile.mkdtemp(prefix="openmixer-ui-drive-")
    xin = XInput()
    app = App(binary, args, workdir)
    try:
        app.wait_ready(ready)
        dr = Driver(xin, app, results, verbose)
        xin.raise_window(app.window)
        time.sleep(0.3)
        # A click on the wordmark (no control) takes the keyboard focus into the window.
        top = min(r["y"] for r in app.layout["controls"].values())
        dr.click_at(20, max(4, top / 4))
        dr.log(f"keyboard focus {hex(xin.focus())}, app window {hex(app.window)}")
        try:
            body(dr)
        finally:
            results.resent += dr.resent
        if not app.alive():
            results.record("the app is still running at the end", False, "running",
                           f"exited {app.proc.returncode}")
    finally:
        xin.release_all()
        app.stop()
        shutil.rmtree(workdir, ignore_errors=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--only", help="run only the check groups whose name starts with this")
    parser.add_argument("--verbose", action="store_true", help="print the values read during drags")
    args = parser.parse_args()
    binary = os.environ.get("OPENMIXER_BIN", "target/debug/openmixer")
    if not sys.platform.startswith("linux"):
        print("ui-drive: Linux only", file=sys.stderr)
        return 2
    if not os.access(binary, os.X_OK):
        print(f"ui-drive: no binary at {binary} (build with --features gui-tauri)", file=sys.stderr)
        return 2
    if subprocess.run(["pgrep", "-x", "openmixer"], capture_output=True).returncode == 0:
        print("ui-drive: another openmixer is running; close it first", file=sys.stderr)
        return 2
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(3))
    results = Results()
    started = time.monotonic()
    try:
        session(binary, ["--demo-library"], both_loaded, lambda dr: suite_loaded(dr, args.only), results,
                args.verbose)
        if not args.only or "disabled".startswith(args.only):
            session(binary, [], any_state, disabled, results)
    except DriveAbort as error:
        print(f"ui-drive: aborted: {error}", file=sys.stderr)
        results.record("the run completed", False, "completed", f"aborted: {error}")
    failed = results.failed()
    total = len(results.rows)
    print(f"SUMMARY  {total - len(failed)}/{total} passed  {len(failed)} failed  "
          f"{results.resent} keys resent  {time.monotonic() - started:.0f} s", flush=True)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
