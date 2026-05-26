use std::path::Path;

pub struct FileHasher;

impl FileHasher {
    pub fn hash_file(path: &Path) -> anyhow::Result<String> {
        let data = std::fs::read(path)?;
        Ok(blake3::hash(&data).to_hex().to_string())
    }

    pub fn hash_bytes(data: &[u8]) -> String {
        blake3::hash(data).to_hex().to_string()
    }
}
