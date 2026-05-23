use serde::{Deserialize, Serialize}
;
#[derive(Debug, Clone, Serialize, Deserialize)]pub struct IndexPreset {
pub name: String,
pub additional_ignore: Vec<String>,
pub watch_extensions: Vec<String>,}
pub fn get_preset(name: &str) -> Option<IndexPreset> {    PRESETS.iter().find(|p| p.name == name).map(|p| p.into())}
const PRESETS: &[IndexPresetStatic] = &[    IndexPresetStatic {        name: "nextjs",        additional_ignore: &[".next/", ".turbo/", "out/"],        watch_extensions: &["ts", "tsx", "js", "jsx", "json", "css", "md"],    }
,    IndexPresetStatic {        name: "rust",        additional_ignore: &["target/", "target-codex-test/"],        watch_extensions: &["rs", "toml"],    }
,    IndexPresetStatic {        name: "tauri",        additional_ignore: &["target/", ".next/", "node_modules/"],        watch_extensions: &["rs", "ts", "tsx", "toml", "json"],    }
,    IndexPresetStatic {        name: "monorepo",        additional_ignore: &["node_modules/", "target/", "dist/", "build/", ".turbo/"],        watch_extensions: &["rs", "ts", "tsx", "js", "py", "json", "toml", "yaml"],    }
,    IndexPresetStatic {        name: "python",        additional_ignore: &["__pycache__/", ".venv/", "venv/", "*.pyc"],        watch_extensions: &["py", "toml", "yaml", "json"],    }
,];
struct IndexPresetStatic {    name: &'static str,    additional_ignore: &'static [&'static str],    watch_extensions: &'static [&'static str],}
impl From<&IndexPresetStatic> for IndexPreset {
fn from(p: &IndexPresetStatic) -> Self {        Self {            name: p.name.to_string(),            additional_ignore: p.additional_ignore.iter().map(|s| s.to_string()).collect(),            watch_extensions: p.watch_extensions.iter().map(|s| s.to_string()).collect(),        }
}
}
pub fn presets() -> Vec<IndexPreset> {    PRESETS.iter().map(|p| IndexPreset::from(p)).collect()}

