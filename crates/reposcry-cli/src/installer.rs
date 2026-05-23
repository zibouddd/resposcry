use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum InstallPlatform {
    /// Claude Code on Linux/macOS
    Claude,
    /// Claude Code on Windows
    Windows,
    /// OpenAI Codex / AGENTS.md style instructions
    Codex,
    /// OpenCode agent instructions
    Opencode,
    /// GitHub Copilot CLI style instructions
    Copilot,
    /// VS Code Copilot Chat project instructions
    Vscode,
    /// Aider project conventions
    Aider,
    /// OpenClaw instructions
    Claw,
    /// Factory Droid instructions
    Droid,
    /// Trae instructions
    Trae,
    /// Trae CN instructions
    #[value(name = "trae-cn")]
    TraeCn,
    /// Gemini CLI GEMINI.md instructions
    Gemini,
    /// Hermes agent instructions
    Hermes,
    /// Kimi Code instructions
    Kimi,
    /// Kiro steering instructions
    Kiro,
    /// Pi coding agent instructions
    Pi,
    /// Cursor project rules
    Cursor,
    /// Google Antigravity instructions
    Antigravity,
    /// Local git/editor hook scripts only
    Hooks,
    /// Install all supported instruction templates
    All,
}

impl InstallPlatform {
    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Windows => "Claude Code Windows",
            Self::Codex => "Codex",
            Self::Opencode => "OpenCode",
            Self::Copilot => "GitHub Copilot CLI",
            Self::Vscode => "VS Code Copilot Chat",
            Self::Aider => "Aider",
            Self::Claw => "OpenClaw",
            Self::Droid => "Factory Droid",
            Self::Trae => "Trae",
            Self::TraeCn => "Trae CN",
            Self::Gemini => "Gemini CLI",
            Self::Hermes => "Hermes",
            Self::Kimi => "Kimi Code",
            Self::Kiro => "Kiro IDE/CLI",
            Self::Pi => "Pi coding agent",
            Self::Cursor => "Cursor",
            Self::Antigravity => "Google Antigravity",
            Self::Hooks => "reposcry hooks",
            Self::All => "all platforms",
        }
    }

    fn slug(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Windows => "claude-windows",
            Self::Codex => "codex",
            Self::Opencode => "opencode",
            Self::Copilot => "copilot",
            Self::Vscode => "vscode",
            Self::Aider => "aider",
            Self::Claw => "claw",
            Self::Droid => "droid",
            Self::Trae => "trae",
            Self::TraeCn => "trae-cn",
            Self::Gemini => "gemini",
            Self::Hermes => "hermes",
            Self::Kimi => "kimi",
            Self::Kiro => "kiro",
            Self::Pi => "pi",
            Self::Cursor => "cursor",
            Self::Antigravity => "antigravity",
            Self::Hooks => "hooks",
            Self::All => "all",
        }
    }

    pub fn concrete_platforms() -> &'static [InstallPlatform] {
        &[
            InstallPlatform::Claude,
            InstallPlatform::Windows,
            InstallPlatform::Codex,
            InstallPlatform::Opencode,
            InstallPlatform::Copilot,
            InstallPlatform::Vscode,
            InstallPlatform::Aider,
            InstallPlatform::Claw,
            InstallPlatform::Droid,
            InstallPlatform::Trae,
            InstallPlatform::TraeCn,
            InstallPlatform::Gemini,
            InstallPlatform::Hermes,
            InstallPlatform::Kimi,
            InstallPlatform::Kiro,
            InstallPlatform::Pi,
            InstallPlatform::Cursor,
            InstallPlatform::Antigravity,
            InstallPlatform::Hooks,
        ]
    }
}

impl fmt::Display for InstallPlatform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.slug())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct InstallOptions {
    pub force: bool,
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub struct InstallWrite {
    pub path: PathBuf,
    pub action: InstallActionTaken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallActionTaken {
    Created,
    Updated,
    Unchanged,
    SkippedExisting,
    DryRunCreate,
    DryRunUpdate,
}

impl fmt::Display for InstallActionTaken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            InstallActionTaken::Created => "created",
            InstallActionTaken::Updated => "updated",
            InstallActionTaken::Unchanged => "unchanged",
            InstallActionTaken::SkippedExisting => "skipped-existing",
            InstallActionTaken::DryRunCreate => "dry-run-create",
            InstallActionTaken::DryRunUpdate => "dry-run-update",
        };
        write!(f, "{}", text)
    }
}

#[derive(Debug, Default)]
pub struct InstallSummary {
    pub writes: Vec<InstallWrite>,
}

impl InstallSummary {
    fn merge(&mut self, other: InstallSummary) {
        self.writes.extend(other.writes);
    }

    fn record(&mut self, path: impl Into<PathBuf>, action: InstallActionTaken) {
        self.writes.push(InstallWrite {
            path: path.into(),
            action,
        });
    }
}

pub fn install_platform(
    repo_root: &Path,
    platform: InstallPlatform,
    options: InstallOptions,
) -> Result<InstallSummary> {
    if platform == InstallPlatform::All {
        let mut combined = InstallSummary::default();
        for platform in InstallPlatform::concrete_platforms() {
            combined.merge(install_platform(repo_root, *platform, options)?);
        }
        return Ok(combined);
    }
    let mut summary = InstallSummary::default();
    install_common_files(repo_root, platform, options, &mut summary)?;
    match platform {
        InstallPlatform::Claude => {
            install_claude(repo_root, false, options, &mut summary)?
        }
        InstallPlatform::Windows => {
            install_claude(repo_root, true, options, &mut summary)?
        }
        InstallPlatform::Codex => {
            install_agents_md(repo_root, platform, options, &mut summary)?
        }
        InstallPlatform::Opencode => {
            install_opencode(repo_root, options, &mut summary)?
        }
        InstallPlatform::Copilot => {
            install_copilot(repo_root, options, &mut summary)?
        }
        InstallPlatform::Vscode => {
            install_vscode(repo_root, options, &mut summary)?
        }
        InstallPlatform::Aider => {
            install_aider(repo_root, options, &mut summary)?
        }
        InstallPlatform::Claw => install_agent_directory(
            repo_root,
            platform,
            ".claw/reposcry.md",
            options,
            &mut summary,
        )?,
        InstallPlatform::Droid => install_agent_directory(
            repo_root,
            platform,
            ".factory/droid/reposcry.md",
            options,
            &mut summary,
        )?,
        InstallPlatform::Trae => install_agent_directory(
            repo_root,
            platform,
            ".trae/rules/reposcry.md",
            options,
            &mut summary,
        )?,
        InstallPlatform::TraeCn => install_agent_directory(
            repo_root,
            platform,
            ".trae-cn/rules/reposcry.md",
            options,
            &mut summary,
        )?,
        InstallPlatform::Gemini => {
            install_gemini(repo_root, options, &mut summary)?
        }
        InstallPlatform::Hermes => install_agent_directory(
            repo_root,
            platform,
            ".hermes/reposcry.md",
            options,
            &mut summary,
        )?,
        InstallPlatform::Kimi => install_agent_directory(
            repo_root,
            platform,
            ".kimi/reposcry.md",
            options,
            &mut summary,
        )?,
        InstallPlatform::Kiro => {
            install_kiro(repo_root, options, &mut summary)?
        }
        InstallPlatform::Pi => install_agent_directory(
            repo_root,
            platform,
            ".pi/reposcry.md",
            options,
            &mut summary,
        )?,
        InstallPlatform::Cursor => {
            install_cursor(repo_root, options, &mut summary)?
        }
        InstallPlatform::Antigravity => install_agent_directory(
            repo_root,
            platform,
            ".antigravity/instructions/reposcry.md",
            options,
            &mut summary,
        )?,
        InstallPlatform::Hooks => {
            install_hooks(repo_root, options, &mut summary)?
        }
        InstallPlatform::All => unreachable!(),
    }
    Ok(summary)
}

fn install_common_files(
    repo_root: &Path,
    platform: InstallPlatform,
    options: InstallOptions,
    summary: &mut InstallSummary,
) -> Result<()> {
    let core = core_agent_instructions(platform);
    write_file(
        repo_root,
        ".reposcry/agents/common.md",
        &core_agent_instructions(InstallPlatform::Claude),
        options,
        summary,
    )?;
    write_file(
        repo_root,
        PathBuf::from(format!(".reposcry/agents/{}.md", platform.slug())),
        &core,
        options,
        summary,
    )?;
    write_file(
        repo_root,
        ".reposcry/skills/code-review-graph/SKILL.md",
        &skill_file(),
        options,
        summary,
    )?;
    write_file(
        repo_root,
        ".reposcry/hooks/pre-edit.md",
        &pre_edit_hook(),
        options,
        summary,
    )?;
    write_file(
        repo_root,
        ".reposcry/hooks/post-edit.md",
        &post_edit_hook(),
        options,
        summary,
    )?;
    write_file(
        repo_root,
        "scripts/reposcry-context.sh",
        &shell_context_script(),
        options,
        summary,
    )?;
    write_file(
        repo_root,
        "scripts/reposcry-validate.sh",
        &shell_validate_script(),
        options,
        summary,
    )?;
    write_file(
        repo_root,
        "scripts/reposcry-context.ps1",
        &powershell_context_script(),
        options,
        summary,
    )?;
    write_file(
        repo_root,
        "scripts/reposcry-validate.ps1",
        &powershell_validate_script(),
        options,
        summary,
    )?;
    upsert_hash_marked_block(
        repo_root,
        ".gitignore",
        "gitignore",
        gitignore_block(),
        options,
        summary,
    )?;
    Ok(())
}

fn install_claude(
    repo_root: &Path,
    windows: bool,
    options: InstallOptions,
    summary: &mut InstallSummary,
) -> Result<()> {
    let platform = if windows {
        InstallPlatform::Windows
    } else {
        InstallPlatform::Claude
    };
    upsert_marked_block(
        repo_root,
        "CLAUDE.md",
        "claude",
        &core_agent_instructions(platform),
        options,
        summary,
    )?;
    write_file(
        repo_root,
        ".claude/commands/reposcry-context.md",
        &claude_command("context", windows),
        options,
        summary,
    )?;
    write_file(
        repo_root,
        ".claude/commands/reposcry-review.md",
        &claude_command("review", windows),
        options,
        summary,
    )?;
    write_file(
        repo_root,
        ".claude/commands/reposcry-validate.md",
        &claude_command("validate", windows),
        options,
        summary,
    )?;
    Ok(())
}

fn install_agents_md(
    repo_root: &Path,
    platform: InstallPlatform,
    options: InstallOptions,
    summary: &mut InstallSummary,
) -> Result<()> {
    upsert_marked_block(
        repo_root,
        "AGENTS.md",
        platform.slug(),
        &core_agent_instructions(platform),
        options,
        summary,
    )
}

fn install_opencode(
    repo_root: &Path,
    options: InstallOptions,
    summary: &mut InstallSummary,
) -> Result<()> {
    install_agents_md(repo_root, InstallPlatform::Opencode, options, summary)?;
    write_file(
        repo_root,
        ".opencode/AGENTS.md",
        &core_agent_instructions(InstallPlatform::Opencode),
        options,
        summary,
    )
}

fn install_copilot(
    repo_root: &Path,
    options: InstallOptions,
    summary: &mut InstallSummary,
) -> Result<()> {
    upsert_marked_block(
        repo_root,
        ".github/copilot-instructions.md",
        "copilot",
        &core_agent_instructions(InstallPlatform::Copilot),
        options,
        summary,
    )
}

fn install_vscode(
    repo_root: &Path,
    options: InstallOptions,
    summary: &mut InstallSummary,
) -> Result<()> {
    install_copilot(repo_root, options, summary)?;
    write_file(
        repo_root,
        ".vscode/reposcry-copilot-instructions.md",
        &core_agent_instructions(InstallPlatform::Vscode),
        options,
        summary,
    )?;
    write_file(
        repo_root,
        ".vscode/settings.json",
        &vscode_settings(),
        InstallOptions {
            force: options.force,
            dry_run: options.dry_run,
        },
        summary,
    )
}

fn install_aider(
    repo_root: &Path,
    options: InstallOptions,
    summary: &mut InstallSummary,
) -> Result<()> {
    write_file(
        repo_root,
        ".aider.reposcry.md",
        &core_agent_instructions(InstallPlatform::Aider),
        options,
        summary,
    )?;
    write_file(
        repo_root,
        ".aider.conf.yml",
        &aider_config(),
        InstallOptions {
            force: options.force,
            dry_run: options.dry_run,
        },
        summary,
    )
}

fn install_gemini(
    repo_root: &Path,
    options: InstallOptions,
    summary: &mut InstallSummary,
) -> Result<()> {
    upsert_marked_block(
        repo_root,
        "GEMINI.md",
        "gemini",
        &core_agent_instructions(InstallPlatform::Gemini),
        options,
        summary,
    )
}

fn install_kiro(
    repo_root: &Path,
    options: InstallOptions,
    summary: &mut InstallSummary,
) -> Result<()> {
    write_file(
        repo_root,
        ".kiro/steering/reposcry.md",
        &core_agent_instructions(InstallPlatform::Kiro),
        options,
        summary,
    )
}

fn install_cursor(
    repo_root: &Path,
    options: InstallOptions,
    summary: &mut InstallSummary,
) -> Result<()> {
    write_file(
        repo_root,
        ".cursor/rules/reposcry-context.mdc",
        &cursor_rule(),
        options,
        summary,
    )?;
    install_agents_md(repo_root, InstallPlatform::Cursor, options, summary)
}

fn install_agent_directory(
    repo_root: &Path,
    platform: InstallPlatform,
    path: &str,
    options: InstallOptions,
    summary: &mut InstallSummary,
) -> Result<()> {
    write_file(
        repo_root,
        path,
        &core_agent_instructions(platform),
        options,
        summary,
    )
}

fn install_hooks(
    repo_root: &Path,
    options: InstallOptions,
    summary: &mut InstallSummary,
) -> Result<()> {
    write_file(
        repo_root,
        ".githooks/pre-commit",
        &git_pre_commit_hook(),
        options,
        summary,
    )?;
    write_file(
        repo_root,
        ".reposcry/hooks/README.md",
        &hooks_readme(),
        options,
        summary,
    )
}

fn write_file(
    repo_root: &Path,
    relative_path: impl AsRef<Path>,
    content: &str,
    options: InstallOptions,
    summary: &mut InstallSummary,
) -> Result<()> {
    let relative_path = relative_path.as_ref();
    let path = repo_root.join(relative_path);
    let content = normalize_newline(content);
    if path.exists() {
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        if existing == content {
            summary.record(relative_path, InstallActionTaken::Unchanged);
            return Ok(());
        }
        if !options.force {
            summary.record(
                relative_path,
                InstallActionTaken::SkippedExisting,
            );
            return Ok(());
        }
        if options.dry_run {
            summary.record(
                relative_path,
                InstallActionTaken::DryRunUpdate,
            );
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, content)?;
        make_executable_if_script(&path)?;
        summary.record(relative_path, InstallActionTaken::Updated);
        return Ok(());
    }
    if options.dry_run {
        summary.record(
            relative_path,
            InstallActionTaken::DryRunCreate,
        );
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, content)?;
    make_executable_if_script(&path)?;
    summary.record(relative_path, InstallActionTaken::Created);
    Ok(())
}

#[allow(unused_variables)]
fn make_executable_if_script(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let is_shell_script =
            path.extension().and_then(|ext| ext.to_str()) == Some("sh");
        let is_git_hook =
            path.file_name().and_then(|name| name.to_str()) == Some("pre-commit");
        if is_shell_script || is_git_hook {
            let mut permissions = std::fs::metadata(path)?.permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(path, permissions)?;
        }
    }
    Ok(())
}

fn upsert_marked_block(
    repo_root: &Path,
    relative_path: impl AsRef<Path>,
    marker: &str,
    block: &str,
    options: InstallOptions,
    summary: &mut InstallSummary,
) -> Result<()> {
    let relative_path = relative_path.as_ref();
    let path = repo_root.join(relative_path);
    let begin = format!("<!-- reposcry:{}:BEGIN -->", marker);
    let end = format!("<!-- reposcry:{}:END -->", marker);
    let managed_block = format!("{}\n{}\n{}\n", begin, block.trim(), end);
    let existing = if path.exists() {
        std::fs::read_to_string(&path).unwrap_or_default()
    } else {
        String::new()
    };
    let next = if let (Some(start), Some(end_pos)) =
        (existing.find(&begin), existing.find(&end))
    {
        let after_end = end_pos + end.len();
        format!(
            "{}{}{}",
            &existing[..start],
            managed_block,
            existing[after_end..].trim_start_matches('\n')
        )
    } else if existing.trim().is_empty() {
        managed_block
    } else {
        format!("{}\n\n{}", existing.trim_end(), managed_block)
    };
    if path.exists() && existing == next {
        summary.record(relative_path, InstallActionTaken::Unchanged);
        return Ok(());
    }
    let action = if path.exists() {
        if options.dry_run {
            InstallActionTaken::DryRunUpdate
        } else {
            InstallActionTaken::Updated
        }
    } else if options.dry_run {
        InstallActionTaken::DryRunCreate
    } else {
        InstallActionTaken::Created
    };
    if !options.dry_run {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, next)?;
    }
    summary.record(relative_path, action);
    Ok(())
}

fn upsert_hash_marked_block(
    repo_root: &Path,
    relative_path: impl AsRef<Path>,
    marker: &str,
    block: &str,
    options: InstallOptions,
    summary: &mut InstallSummary,
) -> Result<()> {
    let relative_path = relative_path.as_ref();
    let path = repo_root.join(relative_path);
    let begin = format!("# reposcry:{}:BEGIN", marker);
    let end = format!("# reposcry:{}:END", marker);
    let managed_block = format!("{}\n{}\n{}\n", begin, block.trim(), end);
    let existing = if path.exists() {
        std::fs::read_to_string(&path).unwrap_or_default()
    } else {
        String::new()
    };
    let next = if let (Some(start), Some(end_pos)) =
        (existing.find(&begin), existing.find(&end))
    {
        let after_end = end_pos + end.len();
        format!(
            "{}{}{}",
            &existing[..start],
            managed_block,
            existing[after_end..].trim_start_matches('\n')
        )
    } else if existing.trim().is_empty() {
        managed_block
    } else {
        format!("{}\n\n{}", existing.trim_end(), managed_block)
    };
    if path.exists() && existing == next {
        summary.record(relative_path, InstallActionTaken::Unchanged);
        return Ok(());
    }
    let action = if path.exists() {
        if options.dry_run {
            InstallActionTaken::DryRunUpdate
        } else {
            InstallActionTaken::Updated
        }
    } else if options.dry_run {
        InstallActionTaken::DryRunCreate
    } else {
        InstallActionTaken::Created
    };
    if !options.dry_run {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, next)?;
    }
    summary.record(relative_path, action);
    Ok(())
}

fn normalize_newline(content: &str) -> String {
    let mut out = content.replace("\r\n", "\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn core_agent_instructions(platform: InstallPlatform) -> String {
    format!(
        r#"# RepoScry instructions for {}

Use reposcry as the local repository map before editing code. \
The goal is to avoid blind edits and avoid sending the full repository into the model context.

## Mandatory workflow before editing

1. Make sure the repo is indexed:

```bash
reposcry index
```

2. Build a focused context pack for the current task:

```bash
reposcry context "$TASK" --strict --budget 20000 --format markdown > .reposcry/AI_CONTEXT.md
```

3. Read `.reposcry/AI_CONTEXT.md` before changing files.

4. For any file you plan to edit, inspect graph impact first:

```bash
reposcry explain path/to/file
reposcry deps path/to/file
reposcry rdeps path/to/file
```

5. After editing, validate the change:

```bash
reposcry validate main...HEAD
```

If the repository does not use `main`, replace `main...HEAD` with the correct base branch.

## Rules for the coding agent

- Do not load the entire repository when reposcry can produce a smaller context pack.
- Do not edit a file only because its name looks relevant. Check dependencies and reverse dependencies first.
- If reposcry reports LOW confidence, do not pretend the context is complete. Search for a better entrypoint or ask for one.
- Prefer `reposcry context`, `reposcry explain`, `reposcry deps`, and `reposcry rdeps` over broad file reads.
- Treat high fan-in files, API boundaries, database layers, event streams, and shared utilities as high-risk.
- Do not edit generated or vendor folders: `target/`, `.next/`, `node_modules/`, `dist/`, `build/`, `public/static/charting_library/`.
- Keep changes minimal and verify with tests or `reposcry validate`.

## Quick commands

```bash
reposcry stats
reposcry context "$TASK" --strict --budget 20000
reposcry report main...HEAD
reposcry rules check
reposcry validate main...HEAD
```"#,
        platform.label()
    )
}

fn skill_file() -> String {
    r#"# Skill: RepoScry assisted coding

Use this skill when you are asked to change, review, refactor, debug, or explain a repository.

## Purpose

reposcry is a local code graph and AI context compiler. It gives the agent a compact map of relevant files, dependencies, reverse dependencies, symbols, tests, risk warnings, and architecture rules.

## Required behavior

Before editing code:

```bash
reposcry index
reposcry context "$TASK" --strict --budget 20000 --format markdown > .reposcry/AI_CONTEXT.md
```

Read `.reposcry/AI_CONTEXT.md`. Then inspect planned edit files:

```bash
reposcry explain <file>
reposcry deps <file>
reposcry rdeps <file>
```

After editing:

```bash
reposcry validate main...HEAD
```

## Anti-token-bloat rule

Never paste the full repository or full graph into context. Ask reposcry for the smallest useful context pack."#
    .to_string()
}

fn pre_edit_hook() -> String {
    r#"# reposcry pre-edit hook

Before making code changes, the agent should run:

```bash
reposcry index
reposcry context "$TASK" --strict --budget 20000 --format markdown > .reposcry/AI_CONTEXT.md
```

Then read `.reposcry/AI_CONTEXT.md` and inspect dependencies for each planned edit file."#
    .to_string()
}

fn post_edit_hook() -> String {
    r#"# reposcry post-edit hook

After making code changes, the agent should run:

```bash
reposcry validate main...HEAD
reposcry report main...HEAD --format markdown > .reposcry/PR_REVIEW.md
```

If validation reports cycles, architecture violations, high-risk files without tests, or low-confidence context, fix or report it before claiming completion."#
    .to_string()
}

fn shell_context_script() -> String {
    r#"#!/usr/bin/env bash
set -euo pipefail
TASK="${*:-Review the current change safely}"
mkdir -p .reposcry
reposcry index
reposcry context "$TASK" --strict --budget "${reposcry_TOKEN_BUDGET:-20000}" --format markdown > .reposcry/AI_CONTEXT.md
printf 'Wrote .reposcry/AI_CONTEXT.md\n'"#
    .to_string()
}

fn shell_validate_script() -> String {
    r#"#!/usr/bin/env bash
set -euo pipefail
BASE="${1:-main...HEAD}"
reposcry validate "$BASE""#
    .to_string()
}

fn powershell_context_script() -> String {
    r#"param(
  [Parameter(ValueFromRemainingArguments = $true)]
  [string[]]$TaskParts
)
$ErrorActionPreference = "Stop"
$Task = if ($TaskParts.Count -gt 0) { $TaskParts -join " " } else { "Review the current change safely" }
$Budget = if ($env:reposcry_TOKEN_BUDGET) { $env:reposcry_TOKEN_BUDGET } else { "20000" }
New-Item -ItemType Directory -Force -Path ".reposcry" | Out-Null
reposcry index
reposcry context $Task --strict --budget $Budget --format markdown | Out-File -Encoding UTF8 ".reposcry/AI_CONTEXT.md"
Write-Host "Wrote .reposcry/AI_CONTEXT.md""#
    .to_string()
}

fn powershell_validate_script() -> String {
    r#"param(
  [string]$Base = "main...HEAD"
)
$ErrorActionPreference = "Stop"
reposcry validate $Base"#
    .to_string()
}

fn gitignore_block() -> &'static str {
    r#"# reposcry local cache and generated reports
.reposcry/
reposcry.db
.reposcry/*.db
.reposcry/*.sqlite
.reposcry/AI_CONTEXT.md
.reposcry/PR_REVIEW.md
.reposcry/*.log"#
}

fn claude_command(kind: &str, windows: bool) -> String {
    let body = match (kind, windows) {
        ("context", true) => r#"Generate a reposcry context pack for the task in $ARGUMENTS.

Run:

```powershell
./scripts/reposcry-context.ps1 $ARGUMENTS
```

Then read `.reposcry/AI_CONTEXT.md` before editing."#,
        ("context", false) => r#"Generate a reposcry context pack for the task in $ARGUMENTS.

Run:

```bash
./scripts/reposcry-context.sh $ARGUMENTS
```

Then read `.reposcry/AI_CONTEXT.md` before editing."#,
        ("review", true) => r#"Review the current branch using reposcry.

Run:

```powershell
reposcry report main...HEAD --format markdown | Out-File -Encoding UTF8 .reposcry/PR_REVIEW.md
```

Read `.reposcry/PR_REVIEW.md` and summarize high-risk changes, impacted files, and tests."#,
        ("review", false) => r#"Review the current branch using reposcry.

Run:

```bash
reposcry report main...HEAD --format markdown > .reposcry/PR_REVIEW.md
```

Read `.reposcry/PR_REVIEW.md` and summarize high-risk changes, impacted files, and tests."#,
        ("validate", true) => r#"Validate the current branch using reposcry.

Run:

```powershell
./scripts/reposcry-validate.ps1 main...HEAD
```

Fix or report any dependency cycles, architecture violations, or missing tests."#,
        _ => r#"Validate the current branch using reposcry.

Run:

```bash
./scripts/reposcry-validate.sh main...HEAD
```

Fix or report any dependency cycles, architecture violations, or missing tests."#,
    };
    body.to_string()
}

fn vscode_settings() -> String {
    r#"{
  "github.copilot.chat.codeGeneration.useInstructionFiles": true
}
"#
    .to_string()
}

fn aider_config() -> String {
    r#"# reposcry generated Aider config.

# If you already have an Aider config, merge this manually instead of forcing overwrite.
read:
  - .aider.reposcry.md
  - .reposcry/skills/code-review-graph/SKILL.md"#
        .to_string()
}

fn cursor_rule() -> String {
    format!(
        r#"---
description: Use RepoScry before edits to avoid blind coding and reduce token usage
globs:
  - "**/*"
alwaysApply: true
---
{}"#,
        core_agent_instructions(InstallPlatform::Cursor)
    )
}

fn git_pre_commit_hook() -> String {
    r#"#!/usr/bin/env bash
set -euo pipefail
if command -v reposcry >/dev/null 2>&1; then
  reposcry validate main...HEAD
else
  echo "reposcry not found; skipping RepoScry validation" >&2
fi"#
    .to_string()
}

fn hooks_readme() -> String {
    r#"# reposcry hooks

This directory contains agent hook instructions and optional git hook scripts.

To enable the generated git hook in this repository:

```bash
git config core.hooksPath .githooks
chmod +x .githooks/pre-commit
```

On Windows PowerShell, run reposcry manually before commit if your Git setup does not execute shell hooks."#
        .to_string()
}
