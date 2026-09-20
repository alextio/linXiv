//! Group `orcid` — the on-demand ORCID backfill pass (`POST /api/orcid/backfill`).

use clap::Subcommand;

use linxiv_core::service::orcid_backfill as svc;
use linxiv_core::service::orcid_backfill::OrcidBackfillResponse;

use crate::ctx::Ctx;
use crate::output::output;

#[derive(Subcommand)]
pub enum OrcidCmd {
    /// Fill missing ORCIDs onto DOI-linked authors via CrossRef then OpenAlex
    Backfill {
        /// Random ORCID-less authors to check this pass (1 to 100)
        #[arg(long, default_value_t = svc::DEFAULT_LIMIT)]
        limit: i64,
    },
}

pub async fn run(cmd: OrcidCmd, ctx: &mut Ctx) -> anyhow::Result<()> {
    let OrcidCmd::Backfill { limit } = cmd;
    svc::validate_limit(limit)?;
    let _guard = svc::try_begin_backfill().ok_or_else(|| anyhow::anyhow!(svc::BUSY_MSG))?;
    let candidates = svc::orcid_backfill_candidates(&ctx.conn, limit)?;
    let (fetched, errored) = svc::fetch_orcid_records(&candidates).await;
    let updated = svc::apply_results(&mut ctx.conn, &candidates, &fetched)?;
    output(&OrcidBackfillResponse {
        checked: candidates.len(),
        updated,
        errored,
    });
    Ok(())
}
