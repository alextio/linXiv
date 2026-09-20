//! Group `versions`: the arXiv new-version monitor (`/api/versions/*`).

use clap::Subcommand;

use linxiv_core::service::version_monitor::{
    self as svc, NewVersionsResponse, VersionCheckResponse,
};

use crate::ctx::Ctx;
use crate::output::{fail, output, resolve_source_fk};

#[derive(Subcommand)]
pub enum VersionsCmd {
    /// Poll arXiv for new versions of the stalest saved papers
    Check {
        /// Papers to poll in this pass (1 to 100)
        #[arg(long, default_value_t = svc::DEFAULT_LIMIT)]
        limit: i64,
    },
    /// List papers with an unacknowledged new version
    New,
    /// Clear the new-version flag on one paper
    Ack { paper_id: String },
}

pub async fn run(cmd: VersionsCmd, ctx: &mut Ctx) -> anyhow::Result<()> {
    match cmd {
        VersionsCmd::Check { limit } => {
            let limit = svc::validate_limit(limit)?;
            let Some(_guard) = svc::try_begin_check() else {
                fail(svc::CHECK_BUSY_MSG)
            };
            let candidates = svc::stale_candidates(&ctx.conn, limit)?;
            let fetched = svc::fetch_latest(&candidates).await?;
            let found = svc::apply_results(&mut ctx.conn, &candidates, &fetched)?;
            output(&VersionCheckResponse {
                checked: candidates.len(),
                new_versions: found,
            });
        }
        VersionsCmd::New => output(&NewVersionsResponse {
            new_versions: svc::list_new_versions(&ctx.conn)?,
        }),
        VersionsCmd::Ack { paper_id } => {
            let fk = resolve_source_fk(&ctx.conn, Some(paper_id))?.expect("id given");
            output(&svc::ack_flagged(&ctx.conn, fk)?);
        }
    }
    Ok(())
}
