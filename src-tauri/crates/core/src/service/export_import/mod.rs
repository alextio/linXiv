//! export_import service.
//!
//! A `.lxproj` file is a zip archive: `manifest.json` (project, papers, notes,
//! annotations — keyed by source_id, no local DB ids) plus optional
//! `pdfs/{source_id}_v{n}.pdf` entries, whose name is owned by
//! [`ArchivePdfName`], NOT by `service::paper::pdf_on_disk_name`.
//!
//! DI seam: every DB-touching fn takes `conn` first, every FS-touching fn its
//! resolved paths. Nothing here reads config.
//!
//! ZIP I/O: `export_project`/`preview_import`/`commit_import` wrap the in-memory
//! manifest layer with `zip` crate reads/writes of the `.lxproj` archive.
//!
//! Rollback: a mid-import failure SOFT-DELETES the project (trash) and surfaces
//! `CoreError::ProjectImport`. Papers saved before the failure stay in the
//! library — the "rollback" is scoped to the project.

mod archive;
mod dto;
mod export;
mod import;
mod share_id;

pub use archive::preview_import;
pub use dto::{ImportPreview, ImportPreviewResponse, ImportedProject, OnConflict};
pub use export::export_project;
pub use import::commit_import;
pub use share_id::valid_share_id;

#[cfg(test)]
mod tests;
