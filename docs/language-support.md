# Language support

RepoScry now recognizes more source file types during scanning.

## Full parser support

These languages currently receive file, symbol, import, call-site, and graph-edge extraction where supported by the existing parsers:

| Language | Extensions | Support |
| --- | --- | --- |
| Rust | `.rs` | symbols, imports, calls |
| TypeScript / TSX | `.ts`, `.tsx` | symbols, imports, calls |
| JavaScript / JSX | `.js`, `.jsx`, `.mjs`, `.cjs` | symbols, imports, calls |
| Python | `.py`, `.pyw` | symbols, imports, calls |
| JSON | `.json` | file metadata |
| TOML | `.toml` | file metadata |
| YAML | `.yaml`, `.yml` | file metadata |

## File-level classification

These languages are indexed at file, path, LOC, and language level. Symbol/import/call extraction can be added later without changing the scanner model.

| Language | Extensions |
| --- | --- |
| Markdown / MDX | `.md`, `.mdx` |
| CSS / Sass / Less | `.css`, `.scss`, `.sass`, `.less` |
| HTML | `.html`, `.htm` |
| SQL | `.sql` |
| Go | `.go` |
| Java | `.java` |
| C# | `.cs` |
| C / C++ | `.c`, `.h`, `.cpp`, `.cc`, `.cxx`, `.hpp`, `.hh`, `.hxx` |
| Kotlin | `.kt`, `.kts` |
| Swift | `.swift` |
| PHP | `.php` |
| Ruby | `.rb` |
| Lua | `.lua` |
| Dart | `.dart` |
| Scala | `.scala`, `.sc` |
| Svelte | `.svelte` |
| Vue | `.vue` |
| Nix | `.nix` |
| PowerShell | `.ps1`, `.psm1`, `.psd1` |

## Next parser priorities

Recommended parser order:

1. Go
2. Java
3. C#
4. Vue/Svelte single-file components
5. Kotlin / Swift
6. SQL-aware dependency extraction
