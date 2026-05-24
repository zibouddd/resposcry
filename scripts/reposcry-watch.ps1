$ErrorActionPreference = "Stop"
$Base = if ($args.Count -gt 0) { $args[0] } else { "main" }
reposcry-watch --repo . --base $Base --refresh-search --skip-warm-calls
