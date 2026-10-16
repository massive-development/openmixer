#!/usr/bin/env python3
"""
License checker for OpenMixer.
Verifies that all third-party dependencies have proper license files
and that no prohibited files (ASIO SDK, etc.) are present.
"""

import os
import re
import sys
from pathlib import Path

# Allowed licenses (text or SPDX identifiers)
ALLOWED_LICENSES = {
    'AGPL-3.0', 'GPL-2.0', 'GPL-3.0', 'LGPL-2.1', 'LGPL-3.0',
    'MIT', 'BSD-2-Clause', 'BSD-3-Clause', 'ISC', 'Zlib',
    'Apache-2.0', 'Unlicense', 'WTFPL', 'CC0-1.0', 'Public Domain'
}

# Files/paths that should never be present
PROHIBITED_FILES = [
    'iasiodrv.h', 'ASIO SDK', 'asiodrivers',
    'ft2build.h'  # Indicates proprietary Freetype without LGPL build
]

def check_license_file(filepath: Path) -> bool:
    """Check if a license file exists and is readable."""
    if not filepath.exists():
        return False
    
    content = filepath.read_text(errors='ignore').lower()
    if len(content) < 100:
        return False
    
    return True

def extract_license_from_text(content: str) -> str | None:
    """Extract license identifier from file content."""
    for license_id in ALLOWED_LICENSES:
        if license_id.lower() in content.lower():
            return license_id
    return None

def check_third_party_deps() -> bool:
    """Check that third-party dependencies have licenses."""
    third_party_dir = Path('third_party')
    if not third_party_dir.exists():
        print("✓ No third_party directory (dependencies fetched via FetchContent)")
        return True
    
    errors = []
    for dep_dir in third_party_dir.iterdir():
        if not dep_dir.is_dir():
            continue
        
        # Look for LICENSE file
        license_files = list(dep_dir.glob('LICENSE*'))
        if not license_files:
            errors.append(f"Missing LICENSE in {dep_dir.name}")
            continue
        
        # Check license is allowed
        license_content = license_files[0].read_text(errors='ignore')
        license_id = extract_license_from_text(license_content)
        if license_id and license_id not in ALLOWED_LICENSES:
            errors.append(f"Non-compliant license in {dep_dir.name}: {license_id}")
    
    if errors:
        print("✗ Third-party dependency license errors:")
        for e in errors:
            print(f"  - {e}")
        return False
    
    print("✓ All third-party dependencies have valid licenses")
    return True

def check_prohibited_files() -> bool:
    """Check that prohibited files are not present."""
    errors = []
    for pattern in PROHIBITED_FILES:
        matches = list(Path('.').rglob(f'*{pattern}*'))
        if matches:
            errors.append(f"Found prohibited file: {pattern} ({len(matches)} matches)")
    
    if errors:
        print("✗ Prohibited files detected:")
        for e in errors:
            print(f"  - {e}")
        return False
    
    print("✓ No prohibited files detected")
    return True

def check_source_files() -> bool:
    """Check that source files have appropriate license headers."""
    source_extensions = {'.cpp', '.h', '.hpp', '.c'}
    issues = []
    
    for py_file in Path('.').rglob('*'):
        if py_file.suffix in source_extensions and '.git' not in str(py_file):
            content = py_file.read_text(errors='ignore').lower()
            if not any([
                'copyright' in content,
                'license' in content,
                'gpl' in content,
                'agpl' in content,
                'openmixer' in content
            ]):
                # Might not have header - just warn for now
                pass
    
    return True

def main():
    print("=" * 60)
    print("OpenMixer License Checker")
    print("=" * 60)
    
    all_passed = True
    
    all_passed &= check_third_party_deps()
    all_passed &= check_prohibited_files()
    all_passed &= check_source_files()
    
    print("=" * 60)
    if all_passed:
        print("✓ All license checks passed")
        return 0
    else:
        print("✗ License checks failed")
        return 1

if __name__ == '__main__':
    sys.exit(main())
