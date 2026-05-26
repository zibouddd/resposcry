param(
  [string]$Base = "main"
)
$ErrorActionPreference = "Stop"
reposcry --repo . detect_changes $Base HEAD --format json
reposcry --repo . validate $Base HEAD
