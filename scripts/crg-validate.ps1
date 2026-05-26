param(
  [string]$Base = "main...HEAD"
)

$ErrorActionPreference = "Stop"
reposcry validate $Base
