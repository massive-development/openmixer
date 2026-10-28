#!/usr/bin/env python3
"""Writes crates/om-stems/tests/fixtures/htdemucs_reference.json: what Demucs itself produces for a
deterministic one-segment signal, so Rust can be checked against the real model.

The signal (sines plus an LCG noise floor) is built the same way in `htdemucs_reference.rs`, bit for
bit in integer arithmetic and to float32 rounding in the sines. The fixture holds

  * `spectrum`: a few samples of `model._spec(mix)` in the complex-as-channels layout the exported
    graph takes (`[channel (re0, im0, re1, im1)][bin][frame]`), to pin `Spectral::forward` to torch;
  * `stem_rms`: per stem and channel RMS of the model's separation of the whole segment, undoing the
    normalisation the way `HtDemucsPipeline` does, to pin the end-to-end result.

Run in a throwaway environment (not part of the build): `pip install torch demucs`, then
`python3 tools/htdemucs_reference.py`. Tested with demucs 4.1.0 and torch 2.14.1 (CPU).
"""
import json
import sys
from pathlib import Path

import numpy as np

SR = 44100
SEG = 343980
FRAMES = [0, 1, 2, 100, 333, 334, 335]
BINS = [0, 1, 2, 5, 40, 100, 400, 2047]


def lcg(seed, n):
    state, out = seed, np.empty(n)
    for i in range(n):
        state = (state * 1664525 + 1013904223) & 0xFFFFFFFF
        out[i] = state / 4294967296.0 * 2.0 - 1.0
    return out


def signal():
    n = np.arange(SEG, dtype=np.float64)
    tau = 2.0 * np.pi
    left = 0.4 * np.sin(tau * 110.0 * n / SR) + 0.2 * np.sin(tau * 1760.0 * n / SR) + 0.05 * lcg(1, SEG)
    right = (0.4 * np.sin(tau * 110.0 * n / SR + 0.3) + 0.2 * np.sin(tau * 880.0 * n / SR)
             + 0.05 * lcg(2, SEG))
    return np.stack([left, right]).astype(np.float32)


def main():
    import torch
    from demucs.pretrained import get_model

    bag = get_model("htdemucs")
    model = bag.models[0] if hasattr(bag, "models") else bag
    model.eval()
    mix = torch.from_numpy(signal())[None]

    with torch.no_grad():
        z = model._magnitude(model._spec(mix))[0].numpy()  # [4, 2048, 336]
        mean, std = mix.mean(), mix.std()
        stems = model((mix - mean) / std) * std + mean  # [1, 4, 2, N], Demucs order

    spectrum = [[[float(z[c, b, f]) for f in FRAMES] for b in BINS] for c in range(4)]
    rms = np.sqrt((stems[0].numpy().astype(np.float64) ** 2).mean(axis=-1))  # [4, 2]
    out = {
        "frames": FRAMES,
        "bins": BINS,
        "spectrum": spectrum,
        "stem_order": list(model.sources),
        "stem_rms": rms.tolist(),
        "demucs": "4.1.0",
    }
    path = Path(__file__).resolve().parents[1] / "crates/om-stems/tests/fixtures/htdemucs_reference.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(out, indent=1) + "\n")
    print(f"wrote {path}", file=sys.stderr)


if __name__ == "__main__":
    main()
