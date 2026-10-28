#!/usr/bin/env python3
"""Export Meta's htdemucs (Demucs v4, hybrid transformer) to ONNX for OpenMixer's stem separator.

STATUS: run against the real htdemucs checkpoint (demucs 4.1.0, torch 2.14.1 CPU, onnx 1.23.2,
onnxscript 0.7.2, opset 17). The graph loads in OpenMixer's `OrtInference` and reproduces Demucs' own
output (see crates/om-stems/tests/htdemucs_reference.rs). The in-graph STFT variant does NOT export
with current PyTorch ("STFT does not currently support complex types"), so this script exports the
STFT-outside variant: Rust computes the spectrogram and inverts the result (`Spectral`).

Not run in CI or by `cargo test`. Needs: python>=3.9, torch, demucs (`pip install demucs`), onnx,
onnxscript. Run it in a throwaway virtualenv.

Usage
  python3 tools/export_htdemucs.py --out htdemucs.onnx
  python3 tools/export_htdemucs.py --manifest-only htdemucs.onnx --url https://host/htdemucs.onnx

Contract (must match `ModelContract::htdemucs_split()` in crates/om-stems/src/contract.rs)
  input  "mix"    float32 [1, 2, 343980]   stereo 44.1 kHz segment, ALREADY normalised by OpenMixer
                                           (mean removed, divided by std; OpenMixer undoes it)
  input  "z"      float32 [1, 4, 2048, 336] STFT of "mix" (n_fft 4096, hop 1024, as Demucs' `_spec`),
                                           complex-as-channels: (left re, left im, right re, right im)
  output "stems"  float32 [1, 4, 2, 343980] time-branch waveform, sources drums, bass, other, vocals
  output "zout"   float32 [1, 4, 4, 2048, 336] frequency-branch spectrum, same channel layout;
                                           OpenMixer inverts it and adds it to "stems"

LICENCE: the Demucs code and htdemucs weights are MIT (Meta Platforms, Inc. and affiliates). The MIT
licence does NOT cover the training data; MUSDB18-HQ is research-licensed. The text below is what the
OpenMixer licence dialog shows and must stay equal to HTDEMUCS_LICENSE_TEXT in
crates/om-stems/src/manifest.rs. Weights are never committed to the repository: host the exported file
yourself and put the printed manifest next to it.
"""
import argparse
import hashlib
import json
import sys
from pathlib import Path

SEGMENT = 343980  # int(7.8 * 44100), Demucs' training segment for htdemucs
MODEL_NAME = "htdemucs"
LICENSE_NAME = "MIT (Demucs code and weights); training data not covered"
LICENSE_TEXT = (
    "htdemucs is Demucs by Meta Platforms, Inc. and affiliates. The Demucs code and the released "
    "weights are published under the MIT licence (see the LICENSE file of the Demucs repository), which "
    "permits use, copying, modification and distribution of the software and the weights, provided the "
    "copyright and permission notice is kept.\n\n"
    "Important: the MIT licence covers the code and the weights as published. It does not grant any "
    "rights in the data the model was trained on. The training set includes MUSDB18-HQ, which is "
    "distributed for research and academic use under its own terms. Those terms are not changed by the "
    "MIT licence of the weights. If you intend to use separated stems commercially, check that your use "
    "is acceptable for yourself. OpenMixer downloads the model to your computer only after you accept "
    "this notice, and never uploads your music."
)


def sha256_of(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def manifest_for(onnx_path: Path, url: str) -> dict:
    """The JSON that `ModelManifest::from_json` reads (camelCase keys), with the default contract."""
    return {
        "id": "htdemucs.onnx",
        "url": url,
        "sha256": sha256_of(onnx_path),
        "sizeBytes": onnx_path.stat().st_size,
        "licenseName": LICENSE_NAME,
        "licenseText": LICENSE_TEXT,
        "contract": {
            "sampleRate": 44100,
            "segmentSamples": SEGMENT,
            "overlap": 0.25,
            "nFft": 4096,
            "hop": 1024,
            "mixInput": "mix",
            "specInput": "z",
            "stemsOutput": "stems",
            "specOutput": "zout",
            "stemOrder": ["drums", "bass", "other", "vocals"],
        },
    }


def export(out: Path, opset: int) -> None:
    import torch  # imported late so --manifest-only and --help work without torch
    from demucs.pretrained import get_model

    bag = get_model(MODEL_NAME)
    model = bag.models[0] if hasattr(bag, "models") else bag
    model.eval()
    if list(model.sources) != ["drums", "bass", "other", "vocals"]:
        sys.exit(f"unexpected source order {model.sources}; update stemOrder in the manifest")

    class Split(torch.nn.Module):
        """(mix, z) -> (stems, zout): Demucs' forward minus its STFT and inverse STFT."""

        def __init__(self, m):
            super().__init__()
            self.m = m

        def forward(self, mix, z):
            m = self.m
            length = mix.shape[-1]
            x = z
            B, _, Fq, T = x.shape
            mean = x.mean(dim=(1, 2, 3), keepdim=True)
            std = x.std(dim=(1, 2, 3), keepdim=True)
            x = (x - mean) / (1e-5 + std)
            xt = mix
            meant = xt.mean(dim=(1, 2), keepdim=True)
            stdt = xt.std(dim=(1, 2), keepdim=True)
            xt = (xt - meant) / (1e-5 + stdt)
            saved, saved_t, lengths, lengths_t = [], [], [], []
            for idx, encode in enumerate(m.encoder):
                lengths.append(x.shape[-1])
                inject = None
                if idx < len(m.tencoder):
                    lengths_t.append(xt.shape[-1])
                    tenc = m.tencoder[idx]
                    xt = tenc(xt)
                    if not tenc.empty:
                        saved_t.append(xt)
                    else:
                        inject = xt
                x = encode(x, inject)
                if idx == 0 and m.freq_emb is not None:
                    frs = torch.arange(x.shape[-2], device=x.device)
                    emb = m.freq_emb(frs).t()[None, :, :, None].expand_as(x)
                    x = x + m.freq_emb_scale * emb
                saved.append(x)
            if m.crosstransformer:
                if m.bottom_channels:
                    b, c, f, t = x.shape
                    x = m.channel_upsampler(x.reshape(b, c, f * t))
                    x = x.reshape(b, x.shape[1], f, t)
                    xt = m.channel_upsampler_t(xt)
                x, xt = m.crosstransformer(x, xt)
                if m.bottom_channels:
                    b, c, f, t = x.shape
                    x = m.channel_downsampler(x.reshape(b, c, f * t))
                    x = x.reshape(b, x.shape[1], f, t)
                    xt = m.channel_downsampler_t(xt)
            for idx, decode in enumerate(m.decoder):
                skip = saved.pop(-1)
                x, pre = decode(x, skip, lengths.pop(-1))
                offset = m.depth - len(m.tdecoder)
                if idx >= offset:
                    tdec = m.tdecoder[idx - offset]
                    length_t = lengths_t.pop(-1)
                    if tdec.empty:
                        xt, _ = tdec(pre[:, :, 0], None, length_t)
                    else:
                        skip = saved_t.pop(-1)
                        xt, _ = tdec(xt, skip, length_t)
            S = len(m.sources)
            zout = x.view(B, S, -1, Fq, T) * std[:, None] + mean[:, None]
            stems = xt.view(B, S, -1, length) * stdt[:, None] + meant[:, None]
            return stems, zout

    if not (model.cac and model.nfft == 4096 and model.hop_length == 1024):
        sys.exit("expected htdemucs' complex-as-channels spectrogram with n_fft 4096, hop 1024")
    frames = -(-SEGMENT // model.hop_length)
    example_mix = torch.zeros(1, 2, SEGMENT)
    example_z = torch.zeros(1, 4, model.nfft // 2, frames)
    # dynamo=False: the legacy TorchScript exporter. The default exporter in recent PyTorch fails on
    # a data-dependent assert in demucs' `pad1d`.
    torch.onnx.export(
        Split(model).eval(),
        (example_mix, example_z),
        str(out),
        input_names=["mix", "z"],
        output_names=["stems", "zout"],
        opset_version=opset,
        do_constant_folding=True,
        dynamo=False,
    )
    print(f"wrote {out}", file=sys.stderr)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--out", type=Path, help="where to write the ONNX file (export mode)")
    ap.add_argument("--opset", type=int, default=17)
    ap.add_argument("--manifest-only", type=Path, metavar="ONNX",
                    help="skip the export; print a manifest (sha256, size) for an existing file")
    ap.add_argument("--url", default="https://example.invalid/htdemucs.onnx",
                    help="where the file will be hosted (goes into the manifest)")
    args = ap.parse_args()

    if args.manifest_only:
        onnx_path = args.manifest_only
    elif args.out:
        export(args.out, args.opset)
        onnx_path = args.out
    else:
        ap.error("give --out FILE to export, or --manifest-only FILE")
    print(json.dumps(manifest_for(onnx_path, args.url), indent=2))


if __name__ == "__main__":
    main()
