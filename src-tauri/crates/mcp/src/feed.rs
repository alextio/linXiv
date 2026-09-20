//! Home feed tools cluster: the arXiv RSS window (fetch + filtered page),
//! per-entry dismissals, and the auto-filter rules.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router, ErrorData};
use schemars::JsonSchema;
use serde::Deserialize;

use linxiv_core::config;
use linxiv_core::error::CoreError;
use linxiv_core::models::OkReceipt;
use linxiv_core::service::feed as svc_feed;
use linxiv_core::service::feed::{CreatedFeedRule, FeedRulesResponse, FilterAction, FilterField};

use crate::util::{core_err, guard_err, invalid, json_ok};
use crate::Server;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetFeedParams {
    /// Feed URL, http(s) only (e.g. "https://rss.arxiv.org/rss/cs.LG").
    pub url: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DismissFeedEntryParams {
    /// Bare arXiv id of the entry (e.g. "2204.12985").
    pub arxiv_id: String,
    /// arXiv version of the entry to dismiss.
    pub version: i64,
    /// Block every version of the paper instead of just this one.
    #[serde(default)]
    pub permanent: bool,
}

/// Entry field a rule matches against, as a schema enum so the tool advertises
/// the valid values.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuleField {
    Title,
    Summary,
    Author,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuleAction {
    Deny,
    Allow,
}

impl From<RuleField> for FilterField {
    fn from(f: RuleField) -> Self {
        match f {
            RuleField::Title => FilterField::Title,
            RuleField::Summary => FilterField::Summary,
            RuleField::Author => FilterField::Author,
        }
    }
}

impl From<RuleAction> for FilterAction {
    fn from(a: RuleAction) -> Self {
        match a {
            RuleAction::Deny => FilterAction::Deny,
            RuleAction::Allow => FilterAction::Allow,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateFeedRuleParams {
    /// Entry field the keywords match against: TITLE, SUMMARY or AUTHOR.
    pub field: RuleField,
    /// Comma-separated keywords.
    pub keywords: String,
    /// DENY hides matching entries (default), ALLOW keeps them.
    #[serde(default)]
    pub action: Option<RuleAction>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RuleIdParams {
    /// Numeric rule id.
    pub rule_id: i64,
}

#[tool_router(router = tools_feed, vis = "pub(crate)")]
impl Server {
    #[tool(
        description = "Fetch an RSS/Atom feed URL (throttled to once per 5 minutes per URL), merge it into the cached window, and return the filtered page with which entries are already saved."
    )]
    pub async fn get_feed(
        &self,
        Parameters(p): Parameters<GetFeedParams>,
    ) -> Result<String, ErrorData> {
        let url = p.url.trim();
        if url.is_empty() {
            return Err(invalid("url is required"));
        }
        let (due, mut title) = svc_feed::throttle_state(url);
        let days = svc_feed::retention_days();
        let mut fetch_err = None;
        if due {
            if let Err(e) = self.with_conn(|conn| svc_feed::prune_dismissed(conn, days)) {
                tracing::warn!(error = %e, "feed prune_dismissed failed");
            }
            // Network step outside the conn; the URL guard runs inside `fetch`.
            match svc_feed::fetch(url, &config::data_dir()).await {
                Ok(f) => {
                    self.with_conn(|conn| svc_feed::apply_fetch(conn, url, &f.entries, days))
                        .map_err(core_err)?;
                    svc_feed::record_fetched(url, &f.title);
                    title = f.title;
                }
                Err(e) => fetch_err = Some(e),
            }
        }
        let page = self
            .with_conn(|conn| svc_feed::read_page(conn, url, days))
            .map_err(core_err)?;
        // Rejected URL (scheme / private host) is the caller's fault; upstream is not.
        json_ok(&page.into_response(title, fetch_err).map_err(|e| match e {
            CoreError::BadRequest(m) => invalid(m),
            other => core_err(other),
        })?)
    }

    #[tool(
        description = "Hide a feed entry by arXiv id and version, or block the whole paper with permanent."
    )]
    pub async fn dismiss_feed_entry(
        &self,
        Parameters(p): Parameters<DismissFeedEntryParams>,
    ) -> Result<String, ErrorData> {
        self.with_conn(|conn| svc_feed::dismiss(conn, &p.arxiv_id, p.version, p.permanent))
            .map_err(guard_err)?;
        json_ok(&OkReceipt { ok: true })
    }

    #[tool(description = "List the feed auto-filter rules.")]
    pub async fn list_feed_rules(&self) -> Result<String, ErrorData> {
        let rules = self
            .with_conn(|conn| svc_feed::list_rules(conn))
            .map_err(core_err)?;
        json_ok(&FeedRulesResponse { rules })
    }

    #[tool(
        description = "Create a feed auto-filter rule: keywords matched against a field, DENY (default) or ALLOW."
    )]
    pub async fn create_feed_rule(
        &self,
        Parameters(p): Parameters<CreateFeedRuleParams>,
    ) -> Result<String, ErrorData> {
        let action = p
            .action
            .map(FilterAction::from)
            .unwrap_or(FilterAction::Deny);
        let rule_id = self
            .with_conn(|conn| svc_feed::create_rule(conn, p.field.into(), &p.keywords, action))
            .map_err(guard_err)?;
        json_ok(&CreatedFeedRule { rule_id })
    }

    #[tool(description = "Delete a feed auto-filter rule by id.")]
    pub async fn delete_feed_rule(
        &self,
        Parameters(p): Parameters<RuleIdParams>,
    ) -> Result<String, ErrorData> {
        self.with_conn(|conn| svc_feed::delete_rule(conn, p.rule_id))
            .map_err(guard_err)?;
        json_ok(&OkReceipt { ok: true })
    }
}
