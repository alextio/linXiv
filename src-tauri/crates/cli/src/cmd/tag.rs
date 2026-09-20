use clap::Subcommand;
use serde_json::json;

use linxiv_core::models::TagIn;
use linxiv_core::service::paper::{self as svc_paper, PaperRef};
use linxiv_core::service::project as svc_project;
use linxiv_core::service::tag::{self as svc_tag, CreatedTag, DeletedTag, PaperTags, Tag};

use crate::ctx::Ctx;
use crate::output::{as_source_id, fail, output};

#[derive(Subcommand)]
pub enum TagCmd {
    // Route parity: `POST /api/papers/{}/tags`.
    /// Add tags to a paper
    Add {
        source_id: String,
        #[arg(required = true, num_args = 1..)]
        tags: Vec<String>,
    },
    // Route parity: `DELETE /api/papers/{}/tags`.
    /// Remove tags from a paper
    Remove {
        source_id: String,
        #[arg(required = true, num_args = 1..)]
        tags: Vec<String>,
    },
    /// List tags on a paper
    List { source_id: String },
    // Route parity: `GET /api/tags`.
    /// List all tags in the database
    ListAll,
    // Route parity: `POST /api/tags`.
    /// Create a tag
    Create { label: String },
    // Route parity: `DELETE /api/tags/{}`.
    /// Delete a tag by ID
    Delete { tag_id: i64 },
    /// Add tags to a project
    AddProject {
        project_id: i64,
        #[arg(required = true, num_args = 1..)]
        tags: Vec<String>,
    },
    /// Remove tags from a project
    RemoveProject {
        project_id: i64,
        #[arg(required = true, num_args = 1..)]
        tags: Vec<String>,
    },
    /// List tags on a project
    ListProject { project_id: i64 },
}

pub async fn run(cmd: TagCmd, ctx: &mut Ctx) -> anyhow::Result<()> {
    match cmd {
        // Prefix the id, UNION tags onto the paper; a miss is not found.
        TagCmd::Add { source_id, tags } => {
            let source_id = as_source_id(&ctx.conn, &source_id);
            let updated = svc_paper::add_paper_tags(&mut ctx.conn, &source_id, &tags)
                .unwrap_or_else(|e| fail(e));
            output(&PaperTags {
                source_id,
                tags: updated,
            });
        }
        TagCmd::Remove { source_id, tags } => {
            let source_id = as_source_id(&ctx.conn, &source_id);
            let updated = svc_paper::remove_paper_tags(&mut ctx.conn, &source_id, &tags)
                .unwrap_or_else(|e| fail(e));
            output(&PaperTags {
                source_id,
                tags: updated,
            });
        }
        // Missing paper -> empty list (no error), matching get_paper_tags.
        TagCmd::List { source_id } => {
            let source_id = as_source_id(&ctx.conn, &source_id);
            let tags = svc_paper::get(&ctx.conn, &PaperRef::source(source_id.clone()))?
                .map(|d| d.tags)
                .unwrap_or_default();
            output(&PaperTags { source_id, tags });
        }
        TagCmd::ListAll => {
            output(&svc_tag::list_all_tags(&ctx.conn)?);
        }
        TagCmd::Create { label } => {
            let tag_id = svc_tag::upsert(
                &mut ctx.conn,
                &TagIn {
                    label: label.clone(),
                },
            )?;
            output(&CreatedTag { tag_id, label });
        }
        TagCmd::Delete { tag_id } => {
            svc_tag::delete(
                &mut ctx.conn,
                &Tag {
                    tag_id: Some(tag_id),
                    label: None,
                },
            )?;
            output(&DeletedTag {
                deleted_tag_id: tag_id,
            });
        }
        // The service fn owns the resolve-or-fail guard.
        TagCmd::AddProject { project_id, tags } => {
            let updated = svc_project::add_project_tags(&mut ctx.conn, project_id, &tags)
                .unwrap_or_else(|e| fail(e));
            output(&json!({ "project_id": project_id, "tags": updated }));
        }
        TagCmd::RemoveProject { project_id, tags } => {
            let updated = svc_project::remove_project_tags(&mut ctx.conn, project_id, &tags)
                .unwrap_or_else(|e| fail(e));
            output(&json!({ "project_id": project_id, "tags": updated }));
        }
        // Tags come off the resolved project's details.
        TagCmd::ListProject { project_id } => {
            let details =
                svc_project::get_required(&ctx.conn, project_id).unwrap_or_else(|e| fail(e));
            output(&json!({ "project_id": project_id, "tags": details.project_tags }));
        }
    }
    Ok(())
}
