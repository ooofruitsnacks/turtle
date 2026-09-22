//! Persistent incremental source index.
//!
//! Stores per-file size, modification time, content hash, and a bounded set of
//! lowercased terms. Unchanged files are never re-read or re-lowercased, so
//! repeated runs on a large project rank candidates without rescanning content.
//!
//! The index is a cache only. Deleting it must never change behavior, and file
//! contents used for edits are always re-read from disk.

use crate::languages;
use anyhow::{Context, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

const INDEX_VERSION: u32 = 1;
const MAX_FILE_BYTES: usize = 256 * 1024;

#[derive(Args, Debug, Clone)]
pub struct IndexOptions {
    #[arg(
        long,
        help = "Disable the persistent source index and rescan every run"
    )]
    pub no_index: bool,

    #[arg(
        long,
        value_name = "PATH",
        help = "Index file location; default is the user cache directory"
    )]
    pub index_path: Option<PathBuf>,

    #[arg(
        long,
        default_value_t = 48,
        value_parser = clap::builder::RangedU64ValueParser::<usize>::new().range(1..=2_000),
        help = "Candidate files loaded per request from the index ranking"
    )]
    pub index_candidates: usize,

    #[arg(
        long,
        default_value_t = 512,
        value_parser = clap::builder::RangedU64ValueParser::<usize>::new().range(16..=20_000),
        help = "Maximum stored terms per file"
    )]
    pub index_terms_per_file: usize,

    #[arg(
        long,
        default_value_t = 20_000,
        value_parser = clap::builder::RangedU64ValueParser::<usize>::new().range(1..=1_000_000),
        help = "Maximum indexed files"
    )]
    pub index_max_files: usize,

    #[arg(
        long,
        default_value_t = 4_000,
        value_parser = clap::builder::RangedU64ValueParser::<usize>::new().range(200..=200_000),
        help = "Maximum bytes of full-project inventory shown to the model"
    )]
    pub index_inventory_bytes: usize,

    #[arg(
        long,
        default_value_t = 2_097_152,
        value_parser = clap::value_parser!(u64).range(4_096..=67_108_864),
        help = "Maximum total bytes of candidate file content loaded per request"
    )]
    pub index_load_bytes: u64,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self {
            no_index: false,
            index_path: None,
            index_candidates: 48,
            index_terms_per_file: 512,
            index_max_files: 20_000,
            index_inventory_bytes: 4_000,
            index_load_bytes: 2_097_152,
        }
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;

    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }

    hash
}

fn cache_root() -> PathBuf {
    if let Ok(explicit) = std::env::var("TURTLE_INDEX_DIR") {
        if !explicit.trim().is_empty() {
            return PathBuf::from(explicit);
        }
    }

    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        if !xdg.trim().is_empty() {
            return PathBuf::from(xdg).join("turtle").join("index");
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return PathBuf::from(home)
                .join(".cache")
                .join("turtle")
                .join("index");
        }
    }

    std::env::temp_dir().join("turtle-index")
}

fn tokenize(text: &str, max_terms: usize) -> Vec<String> {
    let mut terms = BTreeSet::new();

    for word in text.split(|character: char| !character.is_alphanumeric() && character != '_') {
        if word.len() < 3 || word.len() > 64 {
            continue;
        }

        terms.insert(word.to_lowercase());

        if terms.len() >= max_terms {
            break;
        }
    }

    terms.into_iter().collect()
}

fn is_manifest(name_lower: &str) -> bool {
    matches!(
        name_lower,
        "cargo.toml"
            | "rust-toolchain.toml"
            | "rust-toolchain"
            | "pyproject.toml"
            | "setup.cfg"
            | "setup.py"
            | "pytest.ini"
            | "tox.ini"
            | "requirements.txt"
            | "requirements-dev.txt"
            | ".python-version"
            | "package.json"
            | "tsconfig.json"
            | "jsconfig.json"
            | "go.mod"
            | "go.work"
            | "gemfile"
            | "rakefile"
            | ".ruby-version"
            | ".rspec"
            | "cmakelists.txt"
            | "makefile"
            | "gnumakefile"
            | "build.zig"
            | "build.zig.zon"
    ) || name_lower.starts_with("tsconfig.")
        || name_lower.starts_with("vitest.config.")
        || name_lower.starts_with("jest.config.")
        || name_lower.starts_with("eslint.config.")
        || name_lower.starts_with("vite.config.")
}

fn is_test(path_lower: &str, name_lower: &str) -> bool {
    path_lower.starts_with("tests/")
        || path_lower.contains("/tests/")
        || path_lower.starts_with("spec/")
        || path_lower.contains("/spec/")
        || name_lower.starts_with("test_")
        || name_lower.ends_with("_test.go")
        || name_lower.ends_with("_test.rs")
        || name_lower.ends_with("_spec.rb")
        || name_lower.contains(".test.")
        || name_lower.contains(".spec.")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    size: u64,
    mtime_ns: u64,
    hash: u64,
    terms: Vec<String>,
    manifest: bool,
    test: bool,
}

#[derive(Debug, Default)]
pub struct RefreshStats {
    pub indexed: usize,
    pub reread: usize,
    pub removed: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct Persisted {
    version: u32,
    root: String,
    entries: BTreeMap<String, Entry>,
}

pub struct SourceIndex {
    options: IndexOptions,
    path: PathBuf,
    root_label: String,
    entries: BTreeMap<String, Entry>,
    dirty: bool,
}

impl SourceIndex {
    /// Opens (or creates) the index for a project. A corrupt or foreign cache
    /// is discarded rather than trusted.
    pub fn open(root: &Path, options: IndexOptions) -> Result<Self> {
        let canonical = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let root_label = canonical.to_string_lossy().to_string();

        let path = match options.index_path.clone() {
            Some(explicit) => explicit,
            None => cache_root().join(format!("{:016x}.json", fnv1a(root_label.as_bytes()))),
        };

        let entries = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Persisted>(&text) {
                Ok(loaded) if loaded.version == INDEX_VERSION && loaded.root == root_label => {
                    loaded.entries
                }
                _ => BTreeMap::new(),
            },
            Err(_) => BTreeMap::new(),
        };

        Ok(Self {
            options,
            path,
            root_label,
            entries,
            dirty: false,
        })
    }

    pub fn location(&self) -> &Path {
        &self.path
    }

    /// Re-reads only files whose size or modification time changed.
    pub fn refresh(&mut self, root: &Path) -> Result<RefreshStats> {
        let mut stats = RefreshStats::default();
        let mut present = HashSet::new();

        let walk = WalkDir::new(root)
            .follow_links(false)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|entry| {
                entry.depth() == 0
                    || !entry.file_type().is_dir()
                    || !languages::skip_directory(&entry.file_name().to_string_lossy())
            });

        for entry in walk {
            let entry = entry?;

            if !entry.file_type().is_file() || !languages::source_allowed(entry.path()) {
                continue;
            }

            if present.len() >= self.options.index_max_files {
                eprintln!("Source index reached its file-count limit.");
                break;
            }

            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };

            if metadata.len() > MAX_FILE_BYTES as u64 {
                continue;
            }

            let relative = entry
                .path()
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");

            let mtime_ns = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|elapsed| elapsed.as_nanos() as u64)
                .unwrap_or(0);

            present.insert(relative.clone());
            stats.indexed += 1;

            if let Some(existing) = self.entries.get(&relative) {
                if existing.size == metadata.len() && existing.mtime_ns == mtime_ns {
                    continue;
                }
            }

            let mut bytes = Vec::new();

            let opened = std::fs::File::open(entry.path()).and_then(|file| {
                file.take((MAX_FILE_BYTES + 1) as u64)
                    .read_to_end(&mut bytes)
            });

            if opened.is_err() || bytes.len() > MAX_FILE_BYTES {
                continue;
            }

            let Ok(content) = String::from_utf8(bytes) else {
                continue;
            };

            stats.reread += 1;

            let hash = fnv1a(content.as_bytes());
            let path_lower = relative.to_lowercase();

            let name_lower = Path::new(&relative)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();

            // Content identical after a touch: keep the existing terms.
            if let Some(existing) = self.entries.get_mut(&relative) {
                if existing.hash == hash {
                    existing.size = metadata.len();
                    existing.mtime_ns = mtime_ns;
                    self.dirty = true;
                    continue;
                }
            }

            let mut terms = tokenize(&content, self.options.index_terms_per_file);
            terms.extend(tokenize(&path_lower, 32));
            terms.sort();
            terms.dedup();

            self.entries.insert(
                relative,
                Entry {
                    size: metadata.len(),
                    mtime_ns,
                    hash,
                    terms,
                    manifest: is_manifest(&name_lower),
                    test: is_test(&path_lower, &name_lower),
                },
            );

            self.dirty = true;
        }

        let before = self.entries.len();
        self.entries.retain(|path, _| present.contains(path));
        stats.removed = before.saturating_sub(self.entries.len());

        if stats.removed > 0 {
            self.dirty = true;
        }

        Ok(stats)
    }

    /// Ranks indexed files for a query using cached terms only.
    pub fn candidates(&self, query: &str) -> Vec<String> {
        let query_lower = query.to_lowercase();

        let terms: HashSet<String> = query
            .split(|character: char| !character.is_alphanumeric() && character != '_')
            .filter(|word| word.len() >= 3)
            .take(256)
            .map(str::to_lowercase)
            .collect();

        let mut ranked: Vec<(usize, &String)> = self
            .entries
            .iter()
            .map(|(path, entry)| {
                let path_lower = path.to_lowercase();

                let lexical: usize = terms
                    .iter()
                    .map(|term| {
                        usize::from(path_lower.contains(term)) * 12
                            + usize::from(entry.terms.iter().any(|value| value == term))
                    })
                    .sum();

                let explicit = usize::from(query_lower.contains(&path_lower)) * 10_000;
                let manifest = usize::from(entry.manifest) * 200;
                let test = usize::from(entry.test && lexical > 0) * 40;

                (lexical + explicit + manifest + test, path)
            })
            .collect();

        ranked.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(right.1)));

        ranked
            .into_iter()
            .filter(|(score, _)| *score > 0)
            .take(self.options.index_candidates)
            .map(|(_, path)| path.clone())
            .collect()
    }

    /// Full-project inventory, independent of which files were loaded.
    pub fn inventory_block(&self) -> String {
        let mut text = String::from(
            "\nFULL PROJECT INVENTORY from the persistent index. \
             An inventory entry is NOT the file's contents and does not \
             authorize overwriting it.\n",
        );

        let mut shown = 0;

        for path in self.entries.keys() {
            if text.len().saturating_add(path.len()) + 1 > self.options.index_inventory_bytes {
                text.push_str(&format!(
                    "[{} more indexed file(s) omitted]\n",
                    self.entries.len().saturating_sub(shown)
                ));
                break;
            }

            text.push_str(path);
            text.push('\n');
            shown += 1;
        }

        text
    }

    pub fn save(&mut self) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }

        let persisted = Persisted {
            version: INDEX_VERSION,
            root: self.root_label.clone(),
            entries: self.entries.clone(),
        };

        let temporary = self.path.with_extension("json.tmp");
        std::fs::write(&temporary, serde_json::to_vec(&persisted)?)?;
        std::fs::rename(&temporary, &self.path)?;

        self.dirty = false;
        Ok(())
    }

    /// Saving is best effort: a cache failure must not fail the task.
    pub fn save_best_effort(&mut self) {
        if let Err(error) = self.save() {
            eprintln!("Warning: could not persist the source index: {error:#}");
        }
    }
}

/// Loads the exact current contents of selected files, bounded in total size.
pub fn load_sources(
    root: &Path,
    paths: &[String],
    max_total_bytes: u64,
) -> Result<Vec<(String, String)>> {
    let mut loaded = Vec::new();
    let mut total = 0_u64;

    for path in paths {
        let relative = Path::new(path);

        if relative.is_absolute()
            || relative
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            continue;
        }

        if !languages::source_allowed(relative) {
            continue;
        }

        let absolute = root.join(relative);

        match std::fs::symlink_metadata(&absolute) {
            Ok(metadata) if metadata.file_type().is_file() => {}
            _ => continue,
        }

        let mut bytes = Vec::new();

        if std::fs::File::open(&absolute)
            .and_then(|file| {
                file.take((MAX_FILE_BYTES + 1) as u64)
                    .read_to_end(&mut bytes)
            })
            .is_err()
        {
            continue;
        }

        if bytes.len() > MAX_FILE_BYTES {
            continue;
        }

        let Ok(content) = String::from_utf8(bytes) else {
            continue;
        };

        if total.saturating_add(content.len() as u64) > max_total_bytes {
            break;
        }

        total += content.len() as u64;
        loaded.push((path.clone(), content));
    }

    Ok(loaded)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestProject {
        root: PathBuf,
        cache: PathBuf,
        // Keep this alive until the test finishes.
        _directory: tempfile::TempDir,
    }

    impl TestProject {
        fn options(&self) -> IndexOptions {
            IndexOptions {
                index_path: Some(self.cache.clone()),
                ..IndexOptions::default()
            }
        }
    }

    fn project() -> TestProject {
        let directory = tempfile::Builder::new()
            .prefix("turtle-index-test-")
            .tempdir()
            .expect("create unique test directory");

        // Keep the cache outside the tree scanned by refresh().
        let root = directory.path().join("project");
        let cache = directory.path().join("index.json");

        std::fs::create_dir_all(root.join("src")).expect("create source directory");

        std::fs::write(root.join("Cargo.toml"), "[package]\nname=\"demo\"\n")
            .expect("write manifest");

        std::fs::write(
            root.join("src/parser.rs"),
            "pub fn parse_tokens() -> usize { 0 }\n",
        )
        .expect("write parser");

        std::fs::write(root.join("src/unrelated.rs"), "pub fn other() {}\n")
            .expect("write unrelated source");

        TestProject {
            root,
            cache,
            _directory: directory,
        }
    }

    #[test]
    fn rereads_only_changed_files() {
        let fixture = project();
        let root = &fixture.root;
        let mut index = SourceIndex::open(root, fixture.options()).expect("open");

        let first = index.refresh(root).expect("first refresh");
        assert_eq!(first.indexed, 3);
        assert_eq!(first.reread, 3);

        let second = index.refresh(root).expect("second refresh");
        assert_eq!(second.indexed, 3);
        assert_eq!(second.reread, 0, "unchanged files must not be re-read");

        // Change the size as well as the contents so this test does
        // not depend on the filesystem's timestamp precision.
        std::fs::write(
            root.join("src/parser.rs"),
            "pub fn parse_tokens() -> usize { 12345 }\n",
        )
        .expect("modify parser");

        let third = index.refresh(root).expect("third refresh");
        assert_eq!(third.indexed, 3);
        assert_eq!(third.reread, 1);
    }

    #[test]
    fn ranks_query_terms_and_manifests() {
        let fixture = project();
        let root = &fixture.root;
        let mut index = SourceIndex::open(root, fixture.options()).expect("open");

        index.refresh(root).expect("refresh");

        let candidates = index.candidates("fix parse_tokens in src/parser.rs");

        assert_eq!(
            candidates.first().map(String::as_str),
            Some("src/parser.rs")
        );
        assert!(candidates.iter().any(|path| path == "Cargo.toml"));
    }

    #[test]
    fn persists_and_reloads() {
        let fixture = project();
        let root = &fixture.root;
        let mut index = SourceIndex::open(root, fixture.options()).expect("open");

        let first = index.refresh(root).expect("refresh");
        assert_eq!(first.indexed, 3);
        assert_eq!(first.reread, 3);

        index.save().expect("save");
        assert!(fixture.cache.is_file(), "cache must be written");
        drop(index);

        let mut reopened = SourceIndex::open(root, fixture.options()).expect("reopen");
        let stats = reopened.refresh(root).expect("refresh reopened");

        assert_eq!(stats.indexed, 3);
        assert_eq!(stats.reread, 0, "a persisted index must avoid re-reading");
    }

    #[test]
    fn load_sources_respects_byte_budget() {
        let fixture = project();

        let loaded = load_sources(
            &fixture.root,
            &["src/parser.rs".into(), "src/unrelated.rs".into()],
            10,
        )
        .expect("load");

        // Neither fixture file fits within a ten-byte budget.
        assert!(
            loaded.is_empty(),
            "files exceeding the byte budget must not be loaded"
        );
    }
}
