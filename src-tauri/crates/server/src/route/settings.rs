//! `/api/settings` + `PATCH /api/env`. GET: the flat settings object with the
//! mailto env keys overlaid (no wrapper key); PATCH: batch `set`.

use serde::Deserialize;
use serde_json::{Map, Value};

use linxiv_core::config::UserSettings;
use linxiv_core::models::OkReceipt;

use crate::route::{to_value, ApiError, ReqCtx};
use crate::state::AppState;

/// Env values merged into the GET body.
const SETTINGS_ENV_KEYS: [&str; 3] = ["CROSSREF_MAILTO", "OPENALEX_MAILTO", "ARXIV_MAILTO"];

/// The only keys `PATCH /api/env` may set.
const ALLOWED_ENV_KEYS: [&str; 5] = [
    "CROSSREF_MAILTO",
    "OPENALEX_MAILTO",
    "ARXIV_MAILTO",
    "GEMINI_API_KEY",
    "OPENAI_API_KEY",
];

/// Keys `redact_secrets` strips from the GET body, replaced by `<KEY>_SET`.
const SECRET_ENV_KEYS: [&str; 2] = ["GEMINI_API_KEY", "OPENAI_API_KEY"];

pub(crate) async fn handle(_state: &AppState, ctx: &ReqCtx<'_>) -> Option<Result<Value, ApiError>> {
    match (ctx.method, ctx.segs) {
        ("GET", ["api", "settings"]) => Some(get()),
        ("PATCH", ["api", "settings"]) => Some(patch(ctx)),
        ("PATCH", ["api", "env"]) => Some(env_patch(ctx)),
        _ => None,
    }
}

#[derive(Deserialize, ts_rs::TS)]
pub struct EnvPatchBody {
    pub key: String,
    pub value: String,
}

/// `PATCH /api/env` — allowlist-gated (400 otherwise). Sets the live process env
/// var (source clients + the GET overlay read via `std::env::var`) and persists
/// to user settings. `set_var` mutates global process env: a concurrent GET can
/// race it (values are short ASCII — old-or-new, never torn). The persisted copy
/// is NOT reloaded into the env at startup.
fn env_patch(ctx: &ReqCtx<'_>) -> Result<Value, ApiError> {
    let b: EnvPatchBody = ctx.parse_body()?;
    if !ALLOWED_ENV_KEYS.contains(&b.key.as_str()) {
        return Err(ApiError::new(
            400,
            format!("Key '{}' is not settable via this endpoint", b.key),
        ));
    }
    std::env::set_var(&b.key, &b.value);
    UserSettings::load()?.set(b.key, Value::String(b.value))?;
    to_value(&OkReceipt { ok: true })
}

/// `GET /api/settings` — settings first, then each present env key overlaid;
/// missing env keys are skipped. Secrets become `<KEY>_SET` booleans.
fn get() -> Result<Value, ApiError> {
    let secrets: Vec<(&str, bool)> = SECRET_ENV_KEYS
        .iter()
        .map(|&k| (k, std::env::var(k).is_ok_and(|v| !v.is_empty())))
        .collect();
    let settings = redact_secrets(UserSettings::load()?.all(), &secrets);
    let env: Vec<(&str, Option<String>)> = SETTINGS_ENV_KEYS
        .iter()
        .map(|&k| (k, std::env::var(k).ok()))
        .collect();
    Ok(Value::Object(overlay_env(settings, &env)))
}

/// `PATCH /api/settings` — loops `set(k, v)` over the body.
fn patch(ctx: &ReqCtx<'_>) -> Result<Value, ApiError> {
    #[derive(Deserialize)]
    struct Body {
        updates: Map<String, Value>,
    }
    let b: Body = ctx.parse_body()?;
    let mut settings = UserSettings::load()?;
    for (key, value) in b.updates {
        settings.set(key, value)?;
    }
    to_value(&OkReceipt { ok: true })
}

/// Replace each secret key with an appended `<KEY>_SET` bool: true when the
/// stored value or the live env var (`env`) is non-empty.
fn redact_secrets(mut settings: Map<String, Value>, env: &[(&str, bool)]) -> Map<String, Value> {
    for (key, in_env) in env {
        let stored = settings
            .shift_remove(*key)
            .and_then(|v| v.as_str().map(|s| !s.is_empty()))
            .unwrap_or(false);
        settings.insert(format!("{key}_SET"), Value::Bool(stored || *in_env));
    }
    settings
}

/// Overlay present env values onto the settings map. Insert keeps an existing
/// key's position and appends a new one, so merge order is stable.
fn overlay_env(
    mut settings: Map<String, Value>,
    env: &[(&str, Option<String>)],
) -> Map<String, Value> {
    for (key, value) in env {
        if let Some(v) = value {
            settings.insert((*key).to_string(), Value::String(v.clone()));
        }
    }
    settings
}

#[cfg(test)]
mod tests {
    // The GET arm calls UserSettings::load() (real settings file) and the test
    // rule forbids redirecting the data dir, so GET has no isolated test; the
    // redact/overlay helpers — the only nontrivial parts — are pinned below.
    use super::*;
    use serde_json::json;

    #[test]
    fn overlay_appends_present_env_keys_in_order_after_settings() {
        let mut base = Map::new();
        base.insert("theme".into(), json!("dark"));
        let env = [
            ("CROSSREF_MAILTO", Some("a@b.c".to_string())),
            ("OPENALEX_MAILTO", None),
        ];
        let merged = overlay_env(base, &env);
        assert_eq!(
            serde_json::to_string(&Value::Object(merged)).unwrap(),
            r#"{"theme":"dark","CROSSREF_MAILTO":"a@b.c"}"#
        );
    }

    #[test]
    fn redact_replaces_secrets_with_set_flags_keeping_order() {
        let mut base = Map::new();
        base.insert("GEMINI_API_KEY".into(), json!("g"));
        base.insert("theme".into(), json!("dark"));
        base.insert("OPENAI_API_KEY".into(), json!(""));
        let secrets = [("GEMINI_API_KEY", false), ("OPENAI_API_KEY", false)];
        let redacted = redact_secrets(base, &secrets);
        // Then the GET overlay still adds the present mailto key after the flags.
        let env = [
            ("CROSSREF_MAILTO", Some("a@b.c".to_string())),
            ("OPENALEX_MAILTO", None),
        ];
        let merged = overlay_env(redacted, &env);
        assert_eq!(
            serde_json::to_string(&Value::Object(merged)).unwrap(),
            r#"{"theme":"dark","GEMINI_API_KEY_SET":true,"OPENAI_API_KEY_SET":false,"CROSSREF_MAILTO":"a@b.c"}"#
        );
    }

    #[test]
    fn redact_set_flag_true_from_env_or_store_false_when_absent() {
        // Env only, store only, neither: flag never leaks the value.
        let mut base = Map::new();
        base.insert("OPENAI_API_KEY".into(), json!("o"));
        let out = redact_secrets(base, &[("GEMINI_API_KEY", true), ("OPENAI_API_KEY", false)]);
        assert_eq!(out["GEMINI_API_KEY_SET"], json!(true));
        assert_eq!(out["OPENAI_API_KEY_SET"], json!(true));
        assert!(out.get("OPENAI_API_KEY").is_none());
        let out = redact_secrets(Map::new(), &[("GEMINI_API_KEY", false)]);
        assert_eq!(out["GEMINI_API_KEY_SET"], json!(false));
    }

    #[test]
    fn every_allowed_env_key_is_settings_or_secret() {
        // A new ALLOWED_ENV_KEYS entry must land in exactly one sub-array: an
        // unclassified key would either be echoed by GET (if secret) or dropped
        // from the overlay (if a mailto).
        for k in ALLOWED_ENV_KEYS {
            assert!(
                SETTINGS_ENV_KEYS.contains(&k) ^ SECRET_ENV_KEYS.contains(&k),
                "{k} must be in exactly one of SETTINGS_ENV_KEYS / SECRET_ENV_KEYS"
            );
        }
    }

    #[test]
    fn overlay_present_key_updates_in_place_keeping_position() {
        let mut base = Map::new();
        base.insert("CROSSREF_MAILTO".into(), json!("old"));
        base.insert("theme".into(), json!("dark"));
        let env = [
            ("CROSSREF_MAILTO", Some("new".to_string())),
            ("OPENALEX_MAILTO", None),
        ];
        let merged = overlay_env(base, &env);
        assert_eq!(
            serde_json::to_string(&Value::Object(merged)).unwrap(),
            r#"{"CROSSREF_MAILTO":"new","theme":"dark"}"#
        );
    }
}
