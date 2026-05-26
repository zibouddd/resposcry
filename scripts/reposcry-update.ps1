param(
  [string]$Base = "main"
)
$ErrorActionPreference = "Stop"
reposcry-update --repo . --changed --base $Base
