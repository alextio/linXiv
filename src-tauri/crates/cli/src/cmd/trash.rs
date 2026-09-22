use clap::Subcommand;

use linxiv_core::service::paper::{self as svc_paper, PaperRef};
use linxiv_core::service::project::{self as svc_project, Project};

use crate::ctx::Ctx;
use crate::output::{as_source_id, fail, output};

#[derive(Subcommand)]
pub enum TrashCmd {
    // Route parity: `GET /api/trash`.
    /// List soft-deleted papers and projects
    List,
    // Route parity: `POST /api/trash/.../restore`.
    /// Restore a soft-deleted paper
    Restore { source_id: String },
    // Route parity: `DELETE /api/trash/...`.
    /// Permanently delete a paper
    HardDelete {
        source_id: String,
        /// Skip the trash: delete even if the paper is active. Irreversible
        #[arg(long)]
        force: bool,
    },
    // Route parity: `POST /api/trash/projects/{}/restore`.
    /// Restore a soft-deleted project
    RestoreProject { project_id: i64 },
    // Route parity: `DELETE /api/trash/projects/{}`.
    /// Permanently delete a project
    HardDeleteProject {
        project_id: i64,
        /// Skip the trash: delete even if the project is active. Irreversible
        #[arg(long)]
        force: bool,
    },
}

pub async fn run(cmd: TrashCmd, ctx: &mut Ctx) -> anyhow::Result<()> {
    match cmd {
        // The canonical TrashListing envelope (core service::trash).
        TrashCmd::List => {
            output(&linxiv_core::service::trash::list_trash(&ctx.conn)?);
        }
        TrashCmd::Restore { source_id } => {
            let source_id = as_source_id(&ctx.conn, &source_id);
            svc_paper::require_trashed(&ctx.conn, &source_id).unwrap_or_else(|e| fail(e));
            let (pdf_path, project_fks) =
                svc_paper::restore(&mut ctx.conn, &PaperRef::source(source_id.clone()))?;
            output(&linxiv_core::service::trash::RestoredPaper {
                ok: true,
                restored: source_id,
                pdf_path,
                project_fks,
            });
        }
        TrashCmd::HardDelete { source_id, force } => {
            let source_id = as_source_id(&ctx.conn, &source_id);
            if !force {
                svc_paper::require_trashed(&ctx.conn, &source_id).unwrap_or_else(|e| fail(e));
            }
            // get_paper_root None -> not found; unreachable after the guard,
            // but mirror the message off hard_delete's None return.
            if svc_paper::hard_delete(&mut ctx.conn, &PaperRef::source(source_id.clone()))?
                .is_none()
            {
                fail(linxiv_core::error::CoreError::PaperNotFound(
                    source_id.clone(),
                ));
            }
            output(&linxiv_core::service::trash::HardDeletedPaper {
                ok: true,
                hard_deleted: source_id,
            });
        }
        TrashCmd::RestoreProject { project_id } => {
            svc_project::require_trashed(&ctx.conn, project_id).unwrap_or_else(|e| fail(e));
            svc_project::restore(
                &ctx.conn,
                &Project {
                    project_fk: Some(project_id),
                },
            )?;
            output(&linxiv_core::service::trash::RestoredProject {
                ok: true,
                restored_project_id: project_id,
            });
        }
        TrashCmd::HardDeleteProject { project_id, force } => {
            if force {
                svc_project::require(&ctx.conn, project_id).unwrap_or_else(|e| fail(e));
            } else {
                svc_project::require_trashed(&ctx.conn, project_id).unwrap_or_else(|e| fail(e));
            }
            svc_project::hard_delete(
                &mut ctx.conn,
                &Project {
                    project_fk: Some(project_id),
                },
            )?;
            output(&linxiv_core::service::trash::HardDeletedProject {
                ok: true,
                hard_deleted_project_id: project_id,
            });
        }
    }
    Ok(())
}
