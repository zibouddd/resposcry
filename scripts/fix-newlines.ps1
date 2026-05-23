# Restore newlines in Rust source files that got flattened by Set-Content -NoNewline
$files = Get-ChildItem -Recurse -Filter "*.rs" -Path "crates" | Where-Object {
    $lines = (Get-Content $_.FullName).Count
    # Files that were originally much larger than their current line count
    $lines -lt 100 -and $_.Length -gt 500
}

foreach ($file in $files) {
    Write-Host "Fixing: $($file.FullName)"
    $content = Get-Content $file.FullName -Raw

    # Step 1: Add newlines after } that end blocks
    $content = $content -replace '(?<=})\s*(?=pub\s|\#\[|fn |struct |enum |trait |impl |mod |use |const |let |type |unsafe|async|static|macro|#\[)', "`n"

    # Step 2: Add newlines after ; at end of statements (not inside strings or comments)
    $content = $content -replace '(?<=;)\s*(?=pub\s|\#\[|fn |struct |enum |trait |impl |mod |use |const |let |type |unsafe|async|static|macro|#\[|///|//!)', "`n"

    # Step 3: Add newlines before closing braces that have content after them
    $content = $content -replace '(?<=\})\s*(?=\})', "`n"
    $content = $content -replace '(?<=\})\s*(?=\S)', "`n"

    # Step 4: Fix specific patterns
    # After comma + newline-worthy next token
    $content = $content -replace '(?<=,)\s*(?=pub |#\[|fn |struct |enum |trait |impl |mod |use |const )', "`n"

    # Add newline after opening braces for function bodies
    $content = $content -replace '(?<=\{)\s*(?=let |if |for |while |match |return |self\.|Ok)|(?<=\{)\s*(?=pub |fn |struct |//|///)', "`n"

    # Add newline after pub fn, fn, etc that is followed immediately by content
    $content = $content -replace '(?<=\))\s*\{', " {"
    $content = $content -replace '(?<=\})\s*else', "} else"

    # Step 5: Fix indentation - add newlines after { that don't already have one
    # Inline blocks: { let x = ... } -> keep
    # Top-level blocks: struct Foo { field: Type } -> keep inline for struct fields

    Set-Content $file.FullName -Value $content
}

Write-Host "Done. Run 'cargo fmt' to fix formatting."
