param(
  [Parameter(ValueFromRemainingArguments = $true)]
  [string[]]$TaskParts
)

$ErrorActionPreference = "Stop"
$Task = if ($TaskParts.Count -gt 0) { $TaskParts -join " " } else { "Review the current change safely" }
$Budget = if ($env:CRG_TOKEN_BUDGET) { $env:CRG_TOKEN_BUDGET } else { "20000" }
New-Item -ItemType Directory -Force -Path ".code-review-graph" | Out-Null
crg index
crg context $Task --strict --budget $Budget --format markdown | Out-File -Encoding UTF8 ".code-review-graph/AI_CONTEXT.md"
Write-Host "Wrote .code-review-graph/AI_CONTEXT.md"
