//! Group `feed` — the arXiv RSS home feed: one-shot fetch + page, dismissals, filter rules.

use clap::{Subcommand, ValueEnum};

use linxiv_core::config;
use linxiv_core::models::OkReceipt;
use linxiv_core::service::feed as svc_feed;
use linxiv_core::service::feed::{CreatedFeedRule, FeedRulesResponse, FilterAction, FilterField};

use crate::ctx::Ctx;
use crate::output::output;

#[derive(Subcommand)]
pub enum FeedCmd {
    // Route parity: `GET /api/feed`.
    /// Fetch a feed URL, merge it into the cache window, and print the filtered page
    Get { url: String },
    // Route parity: `POST /api/feed/dismiss`.
    /// Hide one entry (this version) or block the whole paper
    Dismiss {
        arxiv_id: String,
        /// arXiv version of the entry to dismiss
        #[arg(long)]
        version: i64,
        /// Block every version of the paper, not just this one
        #[arg(long)]
        permanent: bool,
    },
    /// Manage auto-filter rules
    Rules {
        #[command(subcommand)]
        cmd: RulesCmd,
    },
}

#[derive(Subcommand)]
pub enum RulesCmd {
    // Route parity: `GET /api/feed/rules`.
    /// List auto-filter rules
    List,
    // Route parity: `POST /api/feed/rules`.
    /// Add an auto-filter rule
    Add {
        /// Entry field the keywords match against
        #[arg(long, value_enum)]
        field: Field,
        /// Comma-separated keywords
        #[arg(long)]
        keywords: String,
        /// What to do with a matching entry
        #[arg(long, value_enum, default_value_t = Action::Deny)]
        action: Action,
    },
    // Route parity: `DELETE /api/feed/rules/{}`.
    /// Delete an auto-filter rule by id
    Delete { id: i64 },
}

/// Thin clap mirrors of the core enums (core does not depend on clap).
#[derive(Clone, Copy, ValueEnum)]
#[clap(rename_all = "UPPER")]
pub enum Field {
    Title,
    Summary,
    Author,
}

#[derive(Clone, Copy, ValueEnum)]
#[clap(rename_all = "UPPER")]
pub enum Action {
    Deny,
    Allow,
}

impl From<Field> for FilterField {
    fn from(f: Field) -> Self {
        match f {
            Field::Title => FilterField::Title,
            Field::Summary => FilterField::Summary,
            Field::Author => FilterField::Author,
        }
    }
}

impl From<Action> for FilterAction {
    fn from(a: Action) -> Self {
        match a {
            Action::Deny => FilterAction::Deny,
            Action::Allow => FilterAction::Allow,
        }
    }
}

pub async fn run(cmd: FeedCmd, ctx: &mut Ctx) -> anyhow::Result<()> {
    match cmd {
        // One-shot pass of `GET /api/feed`: prune, fetch, apply, read. The process
        // is short-lived, so the per-URL throttle is skipped.
        FeedCmd::Get { url } => {
            let days = ctx.settings.rss_cache_retention_days();
            if let Err(e) = svc_feed::prune_dismissed(&ctx.conn, days) {
                eprintln!("[feed] prune_dismissed failed: {e}");
            }
            let (title, fetch_err) = match svc_feed::fetch(&url, &config::data_dir()).await {
                Ok(f) => {
                    svc_feed::apply_fetch(&mut ctx.conn, &url, &f.entries, days)?;
                    (f.title, None)
                }
                Err(e) => {
                    eprintln!("[feed] fetch failed for {url}: {e}");
                    (String::new(), Some(e))
                }
            };
            let page = svc_feed::read_page(&mut ctx.conn, &url, days)?;
            output(&page.into_response(title, fetch_err)?);
        }
        FeedCmd::Dismiss {
            arxiv_id,
            version,
            permanent,
        } => {
            svc_feed::dismiss(&ctx.conn, &arxiv_id, version, permanent)?;
            output(&OkReceipt { ok: true });
        }
        FeedCmd::Rules { cmd } => match cmd {
            RulesCmd::List => output(&FeedRulesResponse {
                rules: svc_feed::list_rules(&ctx.conn)?,
            }),
            RulesCmd::Add {
                field,
                keywords,
                action,
            } => {
                let rule_id =
                    svc_feed::create_rule(&ctx.conn, field.into(), &keywords, action.into())?;
                output(&CreatedFeedRule { rule_id });
            }
            RulesCmd::Delete { id } => {
                svc_feed::delete_rule(&ctx.conn, id)?;
                output(&OkReceipt { ok: true });
            }
        },
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct T {
        #[command(subcommand)]
        cmd: FeedCmd,
    }

    /// The clap mirrors accept the wire spellings and map onto the core enums.
    #[test]
    fn rules_add_parses_wire_spellings() {
        let t = T::try_parse_from([
            "t",
            "rules",
            "add",
            "--field",
            "AUTHOR",
            "--keywords",
            "x",
            "--action",
            "ALLOW",
        ])
        .unwrap();
        match t.cmd {
            FeedCmd::Rules {
                cmd: RulesCmd::Add { field, action, .. },
            } => {
                assert_eq!(FilterField::from(field), FilterField::Author);
                assert_eq!(FilterAction::from(action), FilterAction::Allow);
            }
            _ => panic!("wrong arm"),
        }
        assert!(
            T::try_parse_from(["t", "rules", "add", "--field", "BODY", "--keywords", "x"]).is_err()
        );
    }
}
