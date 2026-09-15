//! Per-entity named queries (plan §5.3).
//!
//! NON-NEGOTIABLE notes:
//!   * Every connection these run on already has `PRAGMA foreign_keys = ON`
//!     (storage::db::open) — never open a raw rusqlite::Connection here.
//!   * papers_fts holds one row per version with text, rowid == PAPER_ID;
//!     `source_id` is the lineage string (e.g. "arxiv:2204.12985") the FTS
//!     join runs on: `papers.source_id = papers_fts.source_id`.
//!   * Refresh an FTS entry with DELETE-then-INSERT, never UPDATE.

pub mod annotation;
pub mod author;
pub mod note;
pub mod paper;
pub mod project;
pub mod reading_list;
pub mod rss;
pub mod search;
pub mod search_history;
pub mod search_state;
pub mod tag;
pub mod version_check;

pub use search::search_full_text;
