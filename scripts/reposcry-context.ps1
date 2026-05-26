param(
  [Parameter(ValueFromRemainingArguments = $true)]
  [string[]]$TaskParts
)
$ErrorActionPreference = "Stop"
$Task = if ($TaskParts.Count -gt 0) { $TaskParts -join " " } else { "Review the current change safely" }
$Budget = if ($env:REPOSCRY_TOKEN_BUDGET) { $env:REPOSCRY_TOKEN_BUDGET } else { "20000" }
New-Item -ItemType Directory -Force -Path ".reposcry" | Out-Null
reposcry --repo . index --no-semantic
reposcry --repo . context $Task --strict --budget $Budget --format markdown | Out-File -Encoding UTF8 ".reposcry/AI_CONTEXT.md"
Write-Host "Wrote .reposcry/AI_CONTEXT.md"
