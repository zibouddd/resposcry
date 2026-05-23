use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use reposcry_graph::edge::EdgeKind;
use reposcry_graph::symbol::{Import, Symbol};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFile {
    pub id: i64,
    pub path: String,
    pub language: String,
    pub hash: String,
    pub size_bytes: i64,
    pub loc: i64,
    pub last_indexed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedImport {
    pub id: i64,
    pub file_id: i64,
    pub source: String,
    pub target: String,
    pub is_relative: bool,
    pub imported_names: Vec<String>,
    pub line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedEdge {
    pub id: i64,
    pub source_file_id: i64,
    pub target_file_id: Option<i64>,
    pub target_path: Option<String>,
    pub kind: String,
    pub confidence: f64,
}

pub struct CacheDb {
    conn: Connection,
}

impl CacheDb {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path).context("Failed to open cache database")?;
        let db = Self { conn };
        db.initialize()?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.initialize()?;
        Ok(db)
    }

    fn initialize(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS files (
                id INTEGER PRIMARY KEY,
                path TEXT UNIQUE NOT NULL,
                language TEXT NOT NULL DEFAULT '',
                hash TEXT NOT NULL,
                size_bytes INTEGER NOT NULL DEFAULT 0,
                loc INTEGER NOT NULL DEFAULT 0,
                last_indexed_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS symbols (
                id INTEGER PRIMARY KEY,
                file_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                start_line INTEGER NOT NULL DEFAULT 0,
                end_line INTEGER NOT NULL DEFAULT 0,
                signature TEXT,
                visibility TEXT,
                doc_comment TEXT,
                FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS imports (
                id INTEGER PRIMARY KEY,
                file_id INTEGER NOT NULL,
                source TEXT NOT NULL,
                target TEXT NOT NULL,
                is_relative INTEGER NOT NULL DEFAULT 0,
                imported_names TEXT NOT NULL DEFAULT '[]',
                line INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS edges (
                id INTEGER PRIMARY KEY,
                source_file_id INTEGER NOT NULL,
                target_file_id INTEGER,
                target_path TEXT,
                kind TEXT NOT NULL,
                confidence REAL NOT NULL DEFAULT 1.0,
                FOREIGN KEY (source_file_id) REFERENCES files(id) ON DELETE CASCADE,
                FOREIGN KEY (target_file_id) REFERENCES files(id) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS git_changes (
                id INTEGER PRIMARY KEY,
                path TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'modified',
                lines_added INTEGER NOT NULL DEFAULT 0,
                lines_deleted INTEGER NOT NULL DEFAULT 0,
                recorded_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS config (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL DEFAULT ''
            );
            ",
        )?;
        self.migrate_imports_table()?;
        Ok(())
    }

    fn migrate_imports_table(&self) -> Result<()> {
        let has_imported_names = {
            let mut stmt = self.conn.prepare("PRAGMA table_info(imports)")?;
            let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
            let mut has_imported_names = false;
            for col in columns {
                if col? == "imported_names" {
                    has_imported_names = true;
                    break;
                }
            }
            has_imported_names
        };
        if !has_imported_names {
            self.conn.execute(
                "ALTER TABLE imports ADD COLUMN imported_names TEXT NOT NULL DEFAULT '[]'",
                [],
            )?;
        }
        Ok(())
    }

    pub fn get_file_by_path(&self, path: &str) -> Result<Option<CachedFile>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path, language, hash, size_bytes, loc, last_indexed_at \
             FROM files WHERE path = ?1",
        )?;
        let mut rows = stmt.query_map(params![path], |row| {
            Ok(CachedFile {
                id: row.get(0)?,
                path: row.get(1)?,
                language: row.get(2)?,
                hash: row.get(3)?,
                size_bytes: row.get(4)?,
                loc: row.get(5)?,
                last_indexed_at: row.get(6)?,
            })
        })?;
        match rows.next() {
            Some(Ok(file)) => Ok(Some(file)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    pub fn upsert_file(
        &self,
        path: &str,
        language: &str,
        hash: &str,
        size_bytes: i64,
        loc: i64,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO files (path, language, hash, size_bytes, loc, last_indexed_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now')) \
             ON CONFLICT(path) DO UPDATE SET \
               language = excluded.language, \
               hash = excluded.hash, \
               size_bytes = excluded.size_bytes, \
               loc = excluded.loc, \
               last_indexed_at = datetime('now')",
            params![path, language, hash, size_bytes, loc],
        )?;
        self.get_file_by_path(path)?
            .map(|file| file.id)
            .ok_or_else(|| anyhow::anyhow!("file not found after upsert: {}", path))
    }

    pub fn delete_file(&self, path: &str) -> Result<()> {
        if let Some(file) = self.get_file_by_path(path)? {
            self.conn
                .execute("DELETE FROM files WHERE id = ?1", params![file.id])?;
        }
        Ok(())
    }

    pub fn insert_symbols(&self, file_id: i64, symbols: &[Symbol]) -> Result<()> {
        self.conn
            .execute("DELETE FROM symbols WHERE file_id = ?1", params![file_id])?;
        for sym in symbols {
            self.conn.execute(
                "INSERT INTO symbols (file_id, name, kind, start_line, end_line, signature, visibility, doc_comment) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    file_id,
                    sym.name,
                    sym.kind,
                    sym.start_line,
                    sym.end_line,
                    sym.signature,
                    sym.visibility,
                    sym.doc_comment,
                ],
            )?;
        }
        Ok(())
    }

    pub fn insert_imports(&self, file_id: i64, imports: &[Import]) -> Result<()> {
        self.conn
            .execute("DELETE FROM imports WHERE file_id = ?1", params![file_id])?;
        for import in imports {
            let imported_names = serde_json::to_string(&import.imported_names)?;
            self.conn.execute(
                "INSERT INTO imports (file_id, source, target, is_relative, imported_names, line) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    file_id,
                    import.source,
                    import.target,
                    if import.is_relative { 1 } else { 0 },
                    imported_names,
                    import.line,
                ],
            )?;
        }
        Ok(())
    }

    pub fn get_symbols_by_file(&self, file_id: i64) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.name, s.kind, s.start_line, s.end_line, s.signature, s.visibility, s.doc_comment, f.path \
             FROM symbols s JOIN files f ON s.file_id = f.id WHERE s.file_id = ?1 \
             ORDER BY s.start_line ASC, s.name ASC",
        )?;
        let rows = stmt.query_map(params![file_id], |row| {
            Ok(Symbol {
                id: row.get(0)?,
                file_path: row.get(8)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                start_line: row.get(3)?,
                end_line: row.get(4)?,
                signature: row.get(5)?,
                visibility: row.get(6)?,
                doc_comment: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_imports_by_file(&self, file_id: i64) -> Result<Vec<CachedImport>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, file_id, source, target, is_relative, imported_names, line \
             FROM imports WHERE file_id = ?1 \
             ORDER BY line ASC, target ASC",
        )?;
        let rows = stmt.query_map(params![file_id], |row| {
            let imported_names_json: String = row.get(5)?;
            let imported_names = serde_json::from_str(&imported_names_json).unwrap_or_default();
            Ok(CachedImport {
                id: row.get(0)?,
                file_id: row.get(1)?,
                source: row.get(2)?,
                target: row.get(3)?,
                is_relative: row.get::<_, i64>(4)? != 0,
                imported_names,
                line: row.get::<_, i64>(6)? as u32,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_all_imports(&self) -> Result<Vec<CachedImport>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, file_id, source, target, is_relative, imported_names, line \
             FROM imports \
             ORDER BY file_id ASC, line ASC, target ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let imported_names_json: String = row.get(5)?;
            let imported_names = serde_json::from_str(&imported_names_json).unwrap_or_default();
            Ok(CachedImport {
                id: row.get(0)?,
                file_id: row.get(1)?,
                source: row.get(2)?,
                target: row.get(3)?,
                is_relative: row.get::<_, i64>(4)? != 0,
                imported_names,
                line: row.get::<_, i64>(6)? as u32,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn clear_edges_by_kind(&self, kind: EdgeKind) -> Result<()> {
        self.conn
            .execute("DELETE FROM edges WHERE kind = ?1", params![kind.as_str()])?;
        Ok(())
    }

    pub fn insert_edge(
        &self,
        source_file_id: i64,
        target_file_id: Option<i64>,
        target_path: Option<&str>,
        kind: EdgeKind,
        confidence: f64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO edges (source_file_id, target_file_id, target_path, kind, confidence) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                source_file_id,
                target_file_id,
                target_path,
                kind.as_str(),
                confidence,
            ],
        )?;
        Ok(())
    }

    pub fn get_edges_by_kind(&self, kind: EdgeKind) -> Result<Vec<CachedEdge>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_file_id, target_file_id, target_path, kind, confidence \
             FROM edges WHERE kind = ?1 \
             ORDER BY source_file_id ASC, target_file_id ASC, target_path ASC",
        )?;
        let rows = stmt.query_map(params![kind.as_str()], |row| {
            Ok(CachedEdge {
                id: row.get(0)?,
                source_file_id: row.get(1)?,
                target_file_id: row.get(2)?,
                target_path: row.get(3)?,
                kind: row.get(4)?,
                confidence: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn set_config(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO config (key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_config(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM config WHERE key = ?1")?;
        let mut rows = stmt.query_map(params![key], |row| row.get(0))?;
        match rows.next() {
            Some(Ok(val)) => Ok(Some(val)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    pub fn file_count(&self) -> Result<i64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn symbol_count(&self) -> Result<i64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn import_count(&self) -> Result<i64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM imports", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn edge_count(&self) -> Result<i64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM edges", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn get_all_files(&self) -> Result<Vec<CachedFile>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path, language, hash, size_bytes, loc, last_indexed_at \
             FROM files \
             ORDER BY path ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(CachedFile {
                id: row.get(0)?,
                path: row.get(1)?,
                language: row.get(2)?,
                hash: row.get(3)?,
                size_bytes: row.get(4)?,
                loc: row.get(5)?,
                last_indexed_at: row.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn language_stats(&self) -> Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT language, COUNT(*) as cnt \
             FROM files \
             WHERE language != '' \
             GROUP BY language \
             ORDER BY cnt DESC",
        )?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}
