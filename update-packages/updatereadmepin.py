python3 - <<'PY'
from pathlib import Path

p = Path("README.md")
text = p.read_text()
text = text.replace("REPOSCRY_VERSION=v0.1.0", "REPOSCRY_VERSION=v0.1.2")
text = text.replace("$env:REPOSCRY_VERSION='v0.1.0'", "$env:REPOSCRY_VERSION='v0.1.2'")
text = text.replace("git tag v0.1.0", "git tag v0.1.2")
text = text.replace("git push origin v0.1.0", "git push origin v0.1.2")
p.write_text(text)
PY