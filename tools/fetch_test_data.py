#!/usr/bin/env python3
"""
Fetch test data for OpenMixer.
Downloads small permissively-licensed audio fixtures.
"""

import hashlib
import json
import os
import urllib.request

# Test data manifest
TEST_DATA = {
    "fixtures": {
        "test_tone_440hz.wav": {
            "url": "https://example.org/fixtures/test_tone.wav",  # Placeholder
            "sha256": "placeholder"
        },
        "test_beat_120bpm.wav": {
            "url": "https://example.org/fixtures/test_beat.wav",  # Placeholder
            "sha256": "placeholder"
        }
    }
}

def download_file(url: str, output_path: str, expected_sha256: str | None = None) -> bool:
    """Download a file with optional SHA256 verification."""
    try:
        with urllib.request.urlopen(url, timeout=30) as response:
            data = response.read()
        
        if expected_sha256:
            actual_sha256 = hashlib.sha256(data).hexdigest()
            if actual_sha256 != expected_sha256:
                print(f"✗ SHA256 mismatch for {output_path}")
                return False
        
        with open(output_path, 'wb') as f:
            f.write(data)
        
        print(f"✓ Downloaded: {output_path}")
        return True
    
    except Exception as e:
        print(f"✗ Failed to download {url}: {e}")
        return False

def main():
    fixtures_dir = Path("tests/data/fixtures")
    fixtures_dir.mkdir(parents=True, exist_ok=True)
    
    success = True
    for filename, info in TEST_DATA["fixtures"].items():
        output_path = fixtures_dir / filename
        if output_path.exists():
            print(f"✓ Already exists: {filename}")
            continue
        
        if not download_file(info["url"], str(output_path), info.get("sha256")):
            success = False
    
    if success:
        print("✓ All test data downloaded")
    else:
        print("✗ Some test data downloads failed")

if __name__ == "__main__":
    main()
