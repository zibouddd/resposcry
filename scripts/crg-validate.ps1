param(
  [string]$Base = "main...HEAD"
)

$ErrorActionPreference = "Stop"
crg validate $Base
