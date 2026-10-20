#!/usr/bin/env python3
"""Export Meta's htdemucs (Demucs v4, hybrid transformer) to ONNX for OpenMixer's stem separator.

STATUS: written offline against the Demucs source as remembered; it has NOT been run against a real
htdemucs checkpoint, and the exported graph has not been loaded by OpenMixer. Only --manifest-only is
exercised in tests/by hand. Expect to adjust the export step (torch.stft on complex tensors is the
usual trouble spot; see "Contract" below).

Not run in CI or by `cargo test`. Needs: python>=3.9, torch, demucs (`pip install demucs`), onnx.

Usage
  python3 tools/export_htdemucs.py --out htdemucs.onnx
  python3 tools/export_htdemucs.py --manifest-only htdemucs.onnx --url https://host/htdemucs.onnx

Contract (must match `ModelContract::htdemucs()` in crates/om-stems/src/contract.rs)
  input  "mix"    float32 [1, 2, 343980]   stereo 44.1 kHz segment, ALREADY normalised by OpenMixer
                                           (mean removed, divided by std; OpenMixer undoes it)
  output "stems"  float32 [1, 4, 2, 343980] in Demucs' own source order: drums, bass, other, vocals
  The STFT and inverse STFT are INSIDE the graph (waveform in, waveform out). OpenMixer also supports a
  variant with the STFT outside (inputs "mix" + "z" [1,4,2048,336], outputs "stems" + "zout"
  [1,4,4,2048,336], complex-as-channels, see crates/om-stems/src/spectral.rs), declared through the
  manifest's `contract` object. That variant is not implemented in this script.

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
            "mixInput": "mix",
            "stemsOutput": "stems",
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

    class Wrapper(torch.nn.Module):
        """mix [1,2,N] (normalised) -> stems [1,4,2,N]. Normalisation lives in OpenMixer."""

        def __init__(self, inner):
            super().__init__()
            self.inner = inner

        def forward(self, mix):
            return self.inner(mix)

    example = torch.zeros(1, 2, SEGMENT)
    torch.onnx.export(
        Wrapper(model),
        (example,),
        str(out),
        input_names=["mix"],
        output_names=["stems"],
        opset_version=opset,
        do_constant_folding=True,
    )
    print(f"wrote {out}", file=sys.stderr)
    print(
        "If the export failed on STFT/complex ops, export the time/frequency branches with the STFT "
        "outside the graph instead and declare specInput/specOutput in the manifest contract.",
        file=sys.stderr,
    )


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
