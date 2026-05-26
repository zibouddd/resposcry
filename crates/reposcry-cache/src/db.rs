use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use reposcry_graph::edge::EdgeKind;
use reposcry_graph::symbol::{CallSite, Import, Symbol};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedCallSite {
    pub id: i64,
    pub file_id: i64,
    pub caller: String,
    pub callee: String,
    pub line: u32,
    pub confidence: f64,
    pub resolution_strategy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedSymbolEdge {
    pub id: i64,
    pub source_symbol_id: i64,
    pub target_symbol_id: i64,
    pub source_file_id: i64,
    pub target_file_id: i64,
    pub kind: String,
    pub line: u32,
    pub confidence: f64,
    pub resolution_strategy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedSearchHit {
    pub node_id: i64,
    pub file_path: String,
    pub kind: String,
    pub name: String,
    pub signature: Option<String>,
    pub score: f64,
    pub match_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedSearchVector {
    pub node_id: i64,
    pub file_path: String,
    pub kind: String,
    pub name: String,
    pub signature: Option<String>,
    pub backend: String,
    pub dims: u32,
    pub vector: Vec<f32>,
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
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA cache_size = -64000;
            PRAGMA temp_store = MEMORY;
            PRAGMA busy_timeout = 5000;
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
            CREATE TABLE IF NOT EXISTS call_sites (
                id INTEGER PRIMARY KEY,
                file_id INTEGER NOT NULL,
                caller TEXT NOT NULL,
                callee TEXT NOT NULL,
                line INTEGER NOT NULL DEFAULT 0,
                confidence REAL NOT NULL DEFAULT 1.0,
                resolution_strategy TEXT,
                FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS symbol_edges (
                id INTEGER PRIMARY KEY,
                source_symbol_id INTEGER NOT NULL,
                target_symbol_id INTEGER NOT NULL,
                source_file_id INTEGER NOT NULL,
                target_file_id INTEGER NOT NULL,
                kind TEXT NOT NULL,
                line INTEGER NOT NULL DEFAULT 0,
                confidence REAL NOT NULL DEFAULT 1.0,
                resolution_strategy TEXT,
                FOREIGN KEY (source_symbol_id) REFERENCES symbols(id) ON DELETE CASCADE,
                FOREIGN KEY (target_symbol_id) REFERENCES symbols(id) ON DELETE CASCADE,
                FOREIGN KEY (source_file_id) REFERENCES files(id) ON DELETE CASCADE,
                FOREIGN KEY (target_file_id) REFERENCES files(id) ON DELETE CASCADE
            );
            CREATE VIRTUAL TABLE IF NOT EXISTS search_index USING fts5(
                node_id UNINDEXED,
                file_path,
                kind,
                name,
                signature,
                doc_comment,
                imports,
                content
            );
            CREATE TABLE IF NOT EXISTS search_vectors (
                node_id INTEGER NOT NULL,
                file_path TEXT NOT NULL,
                kind TEXT NOT NULL,
                name TEXT NOT NULL,
                signature TEXT,
                backend TEXT NOT NULL,
                dims INTEGER NOT NULL,
                vector BLOB NOT NULL,
                PRIMARY KEY (node_id, backend)
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
            CREATE INDEX IF NOT EXISTS idx_symbols_file_id ON symbols(file_id);
            CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
            CREATE INDEX IF NOT EXISTS idx_symbols_kind ON symbols(kind);
            CREATE INDEX IF NOT EXISTS idx_imports_file_id ON imports(file_id);
            CREATE INDEX IF NOT EXISTS idx_imports_target ON imports(target);
            CREATE INDEX IF NOT EXISTS idx_edges_kind ON edges(kind);
            CREATE INDEX IF NOT EXISTS idx_edges_source_file_id ON edges(source_file_id);
            CREATE INDEX IF NOT EXISTS idx_edges_target_file_id ON edges(target_file_id);
            CREATE INDEX IF NOT EXISTS idx_call_sites_file_id ON call_sites(file_id);
            CREATE INDEX IF NOT EXISTS idx_call_sites_callee ON call_sites(callee);
            CREATE INDEX IF NOT EXISTS idx_symbol_edges_kind ON symbol_edges(kind);
            CREATE INDEX IF NOT EXISTS idx_symbol_edges_source_file_id ON symbol_edges(source_file_id);
            CREATE INDEX IF NOT EXISTS idx_symbol_edges_target_file_id ON symbol_edges(target_file_id);
            CREATE INDEX IF NOT EXISTS idx_search_vectors_backend_kind ON search_vectors(backend, kind);
            CREATE INDEX IF NOT EXISTS idx_git_changes_path ON git_changes(path);
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
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM symbols WHERE file_id = ?1", params![file_id])?;
        for sym in symbols {
            tx.execute(
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
        tx.commit()?;
        Ok(())
    }

    pub fn insert_imports(&self, file_id: i64, imports: &[Import]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM imports WHERE file_id = ?1", params![file_id])?;
        for import in imports {
            let imported_names = serde_json::to_string(&import.imported_names)?;
            tx.execute(
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
        tx.commit()?;
        Ok(())
    }

    pub fn insert_call_sites(&self, file_id: i64, call_sites: &[CallSite]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM call_sites WHERE file_id = ?1",
            params![file_id],
        )?;
        for call_site in call_sites {
            tx.execute(
                "INSERT INTO call_sites (file_id, caller, callee, line, confidence, resolution_strategy) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    file_id,
                    call_site.caller,
                    call_site.callee,
                    call_site.line,
                    call_site.confidence,
                    call_site.resolution_strategy,
                ],
            )?;
        }
        tx.commit()?;
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

    pub fn get_call_sites_by_file(&self, file_id: i64) -> Result<Vec<CachedCallSite>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, file_id, caller, callee, line, confidence, resolution_strategy \
             FROM call_sites WHERE file_id = ?1 \
             ORDER BY line ASC, callee ASC",
        )?;
        let rows = stmt.query_map(params![file_id], |row| {
            Ok(CachedCallSite {
                id: row.get(0)?,
                file_id: row.get(1)?,
                caller: row.get(2)?,
                callee: row.get(3)?,
                line: row.get::<_, i64>(4)? as u32,
                confidence: row.get(5)?,
                resolution_strategy: row.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_all_call_sites(&self) -> Result<Vec<CachedCallSite>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, file_id, caller, callee, line, confidence, resolution_strategy \
             FROM call_sites \
             ORDER BY file_id ASC, line ASC, callee ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(CachedCallSite {
                id: row.get(0)?,
                file_id: row.get(1)?,
                caller: row.get(2)?,
                callee: row.get(3)?,
                line: row.get::<_, i64>(4)? as u32,
                confidence: row.get(5)?,
                resolution_strategy: row.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn clear_edges_by_kind(&self, kind: EdgeKind) -> Result<()> {
        self.conn
            .execute("DELETE FROM edges WHERE kind = ?1", params![kind.as_str()])?;
        Ok(())
    }

    pub fn clear_symbol_edges_by_kind(&self, kind: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM symbol_edges WHERE kind = ?1", params![kind])?;
        Ok(())
    }

    pub fn delete_edges_by_source(&self, source_file_id: i64, kind: EdgeKind) -> Result<()> {
        self.conn.execute(
            "DELETE FROM edges WHERE source_file_id = ?1 AND kind = ?2",
            params![source_file_id, kind.as_str()],
        )?;
        Ok(())
    }

    pub fn delete_symbol_edges_by_source(&self, source_file_id: i64, kind: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM symbol_edges WHERE source_file_id = ?1 AND kind = ?2",
            params![source_file_id, kind],
        )?;
        Ok(())
    }

    pub fn clear_search_index(&self) -> Result<()> {
        self.conn.execute("DELETE FROM search_index", [])?;
        Ok(())
    }

    pub fn clear_search_vectors(&self, backend: Option<&str>) -> Result<()> {
        match backend {
            Some(backend) => {
                self.conn.execute(
                    "DELETE FROM search_vectors WHERE backend = ?1",
                    params![backend],
                )?;
            }
            None => {
                self.conn.execute("DELETE FROM search_vectors", [])?;
            }
        }
        Ok(())
    }

    pub fn has_search_vector(&self, node_id: i64, backend: &str) -> Result<bool> {
        let mut stmt = self.conn.prepare(
            "SELECT 1 FROM search_vectors WHERE node_id = ?1 AND backend = ?2 LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![node_id, backend], |row| row.get::<_, i64>(0))?;
        match rows.next() {
            Some(Ok(_)) => Ok(true),
            Some(Err(error)) => Err(error.into()),
            None => Ok(false),
        }
    }

    pub fn prune_search_vectors_to_index(&self, backend: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM search_vectors \
             WHERE backend = ?1 \
             AND node_id NOT IN (SELECT CAST(node_id AS INTEGER) FROM search_index)",
            params![backend],
        )?;
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

    pub fn insert_symbol_edges(&self, edges: &[CachedSymbolEdge]) -> Result<()> {
        if edges.is_empty() {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        for edge in edges {
            tx.execute(
                "INSERT INTO symbol_edges (source_symbol_id, target_symbol_id, source_file_id, target_file_id, kind, line, confidence, resolution_strategy) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    edge.source_symbol_id,
                    edge.target_symbol_id,
                    edge.source_file_id,
                    edge.target_file_id,
                    edge.kind,
                    edge.line,
                    edge.confidence,
                    edge.resolution_strategy,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_symbol_edges_by_kind(&self, kind: &str) -> Result<Vec<CachedSymbolEdge>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_symbol_id, target_symbol_id, source_file_id, target_file_id, kind, line, confidence, resolution_strategy \
             FROM symbol_edges WHERE kind = ?1 \
             ORDER BY source_symbol_id ASC, target_symbol_id ASC, line ASC",
        )?;
        let rows = stmt.query_map(params![kind], |row| {
            Ok(CachedSymbolEdge {
                id: row.get(0)?,
                source_symbol_id: row.get(1)?,
                target_symbol_id: row.get(2)?,
                source_file_id: row.get(3)?,
                target_file_id: row.get(4)?,
                kind: row.get(5)?,
                line: row.get::<_, i64>(6)? as u32,
                confidence: row.get(7)?,
                resolution_strategy: row.get(8)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn insert_search_document(
        &self,
        node_id: i64,
        file_path: &str,
        kind: &str,
        name: &str,
        signature: Option<&str>,
        doc_comment: Option<&str>,
        imports: &str,
        content: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO search_index (node_id, file_path, kind, name, signature, doc_comment, imports, content) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                node_id,
                file_path,
                kind,
                name,
                signature,
                doc_comment,
                imports,
                content,
            ],
        )?;
        Ok(())
    }

    pub fn insert_search_vector(
        &self,
        node_id: i64,
        file_path: &str,
        kind: &str,
        name: &str,
        signature: Option<&str>,
        backend: &str,
        vector: &[f32],
    ) -> Result<()> {
        let mut bytes = Vec::with_capacity(vector.len() * std::mem::size_of::<f32>());
        for value in vector {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        self.conn.execute(
            "INSERT OR REPLACE INTO search_vectors \
             (node_id, file_path, kind, name, signature, backend, dims, vector) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                node_id,
                file_path,
                kind,
                name,
                signature,
                backend,
                i64::try_from(vector.len()).unwrap_or(0),
                bytes,
            ],
        )?;
        Ok(())
    }

    pub fn search_nodes_fts(
        &self,
        query: &str,
        kind: Option<&str>,
        limit: usize,
    ) -> Result<Vec<CachedSearchHit>> {
        let limit = i64::try_from(limit).unwrap_or(50);
        let hits = if let Some(kind) = kind {
            let mut stmt = self.conn.prepare(
                "SELECT node_id, file_path, kind, name, signature, bm25(search_index) \
                 FROM search_index \
                 WHERE search_index MATCH ?1 AND kind = ?2 \
                 ORDER BY bm25(search_index) \
                 LIMIT ?3",
            )?;
            let rows = stmt.query_map(params![query, kind, limit], |row| {
                let score: f64 = row.get(5)?;
                Ok(CachedSearchHit {
                    node_id: row.get(0)?,
                    file_path: row.get(1)?,
                    kind: row.get(2)?,
                    name: row.get(3)?,
                    signature: row.get(4)?,
                    score: -score,
                    match_reason: "fts5".to_string(),
                })
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT node_id, file_path, kind, name, signature, bm25(search_index) \
                 FROM search_index \
                 WHERE search_index MATCH ?1 \
                 ORDER BY bm25(search_index) \
                 LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![query, limit], |row| {
                let score: f64 = row.get(5)?;
                Ok(CachedSearchHit {
                    node_id: row.get(0)?,
                    file_path: row.get(1)?,
                    kind: row.get(2)?,
                    name: row.get(3)?,
                    signature: row.get(4)?,
                    score: -score,
                    match_reason: "fts5".to_string(),
                })
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        Ok(hits)
    }

    pub fn get_search_vectors(
        &self,
        backend: &str,
        kind: Option<&str>,
    ) -> Result<Vec<CachedSearchVector>> {
        let query = match kind {
            Some(_) => {
                "SELECT node_id, file_path, kind, name, signature, backend, dims, vector \
                 FROM search_vectors WHERE backend = ?1 AND kind = ?2"
            }
            None => {
                "SELECT node_id, file_path, kind, name, signature, backend, dims, vector \
                 FROM search_vectors WHERE backend = ?1"
            }
        };
        let mut stmt = self.conn.prepare(query)?;
        let map_row = |row: &rusqlite::Row<'_>| {
            let blob: Vec<u8> = row.get(7)?;
            let mut vector = Vec::with_capacity(blob.len() / 4);
            for chunk in blob.chunks_exact(4) {
                vector.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
            Ok(CachedSearchVector {
                node_id: row.get(0)?,
                file_path: row.get(1)?,
                kind: row.get(2)?,
                name: row.get(3)?,
                signature: row.get(4)?,
                backend: row.get(5)?,
                dims: row.get::<_, i64>(6)? as u32,
                vector,
            })
        };
        let rows = match kind {
            Some(kind) => stmt.query_map(params![backend, kind], map_row)?,
            None => stmt.query_map(params![backend], map_row)?,
        };
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
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

    pub fn call_site_count(&self) -> Result<i64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM call_sites", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn symbol_edge_count(&self) -> Result<i64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM symbol_edges", [], |row| row.get(0))?;
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
