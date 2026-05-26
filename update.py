from pathlib import Path
import os
import re


for manifest in Path("crates").glob("*/Cargo.toml"):
    text = manifest.read_text()

    # Update this crate's own package version.
    text = re.sub(
        r'(?m)^version\s*=\s*"[^"]+"',
        f'version = "0.1.2"',
        text,
        count=1,
    )

    # Update internal RepoScry dependency versions.
    text = re.sub(
        r'(reposcry-[a-z-]+\s*=\s*\{\s*version\s*=\s*)"[^"]+"',
        lambda m: f'{m.group(1)}"0.1.2"',
        text,
    )

    manifest.write_text(text)