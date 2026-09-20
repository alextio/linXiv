//! Verifiable citation handles: `linxiv://paper/{source_fk}?v={version}`.
//! A client pastes the handle into notes; `resolve_refs` later tells it
//! whether each one still points at a stored, current version.

use std::fmt;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::storage::queries::paper as store;

const PREFIX: &str = "linxiv://paper/";

/// One citation handle: a paper root (SOURCE_FK) pinned to a stored version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaperRef {
    pub source_fk: i64,
    pub version: i64,
}

impl PaperRef {
    /// Exactly `linxiv://paper/{source_fk}?v={version}`, both positive integers.
    pub fn parse(s: &str) -> Result<PaperRef> {
        let bad = || CoreError::BadRequest(format!("malformed paper ref: {s}"));
        let rest = s.strip_prefix(PREFIX).ok_or_else(bad)?;
        let (fk, v) = rest.split_once("?v=").ok_or_else(bad)?;
        let source_fk: i64 = fk.parse().map_err(|_| bad())?;
        let version: i64 = v.parse().map_err(|_| bad())?;
        if source_fk < 1 || version < 1 {
            return Err(bad());
        }
        Ok(PaperRef { source_fk, version })
    }
}

impl fmt::Display for PaperRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{PREFIX}{}?v={}", self.source_fk, self.version)
    }
}

/// Resolution outcome for one ref.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RefStatus {
    /// The cited version is the latest stored one.
    Current,
    /// The paper exists but a newer version is stored.
    Stale,
    /// No paper with that source_fk, or that version was never stored.
    Unknown,
    /// Not a `linxiv://paper/{source_fk}?v={version}` string.
    Malformed,
}

/// `resolve_refs` row: the input echoed with what the library knows about it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefResolution {
    pub r#ref: String,
    pub status: RefStatus,
    pub source_id: Option<String>,
    pub title: Option<String>,
    pub latest_version: Option<i64>,
}

/// `paper_ref` envelope: the handle for one stored version of a paper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperRefOut {
    pub r#ref: String,
    pub source_id: String,
    pub source_fk: i64,
    pub version: i64,
}

/// Handle for `source_id` at `version` (latest stored when `None`);
/// `PaperNotFound` when the paper or that version is not stored.
pub fn paper_ref(conn: &Connection, source_id: &str, version: Option<i64>) -> Result<PaperRefOut> {
    let source_fk = crate::service::paper::resolve_source_fk(conn, source_id)?;
    let sid = store::get_source_id(conn, source_fk)?
        .ok_or_else(|| CoreError::PaperNotFound(source_id.to_string()))?;
    let paper = store::get_paper(conn, &sid, version)?
        .ok_or_else(|| CoreError::PaperNotFound(source_id.to_string()))?;
    Ok(PaperRefOut {
        r#ref: PaperRef {
            source_fk,
            version: paper.version,
        }
        .to_string(),
        source_id: sid,
        source_fk,
        version: paper.version,
    })
}

/// Resolve each ref against the active library, one row per input in order.
/// Trashed papers resolve as `unknown`: nothing a client can open until the
/// paper is restored, and restoring makes the same ref `current` again.
pub fn resolve_refs(conn: &Connection, refs: &[String]) -> Result<Vec<RefResolution>> {
    refs.iter().map(|r| resolve_one(conn, r)).collect()
}

fn resolve_one(conn: &Connection, r: &str) -> Result<RefResolution> {
    let mut out = RefResolution {
        r#ref: r.to_string(),
        status: RefStatus::Malformed,
        source_id: None,
        title: None,
        latest_version: None,
    };
    let Ok(pr) = PaperRef::parse(r) else {
        return Ok(out);
    };
    out.status = RefStatus::Unknown;
    let Some(sid) = store::get_source_id(conn, pr.source_fk)? else {
        return Ok(out);
    };
    // `papers` hides trashed roots, so a trashed paper falls out here as unknown.
    let Some(latest) = store::get_paper(conn, &sid, None)? else {
        return Ok(out);
    };
    out.source_id = Some(sid.clone());
    out.latest_version = Some(latest.version);
    let Some(cited) = store::get_paper(conn, &sid, Some(pr.version))? else {
        return Ok(out);
    };
    out.title = Some(cited.title);
    out.status = if cited.version == latest.version {
        RefStatus::Current
    } else {
        RefStatus::Stale
    };
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::paper as svc_paper;
    use crate::test_support::{db, meta};

    #[test]
    fn parse_display_round_trip_and_rejections() {
        let pr = PaperRef::parse("linxiv://paper/7?v=2").unwrap();
        assert_eq!(
            pr,
            PaperRef {
                source_fk: 7,
                version: 2
            }
        );
        assert_eq!(pr.to_string(), "linxiv://paper/7?v=2");
        for bad in [
            "",
            "linxiv://paper/7",
            "linxiv://paper/7?v=",
            "linxiv://paper/?v=2",
            "linxiv://paper/7?v=0",
            "linxiv://paper/0?v=1",
            "linxiv://paper/7?v=2&x=1",
            "linxiv://paper/abc?v=2",
            "arxiv:2204.12985",
            "http://linxiv/paper/7?v=2",
        ] {
            assert!(PaperRef::parse(bad).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn resolves_one_case_per_status() {
        let mut conn = db();
        svc_paper::save_paper_metadata(&mut conn, &meta("arxiv:A", 1), None).unwrap();
        svc_paper::save_paper_metadata(&mut conn, &meta("arxiv:A", 2), None).unwrap();
        svc_paper::save_paper_metadata(&mut conn, &meta("arxiv:B", 1), None).unwrap();
        let a = svc_paper::resolve_source_fk(&conn, "arxiv:A").unwrap();
        let b = svc_paper::resolve_source_fk(&conn, "arxiv:B").unwrap();
        svc_paper::delete(&mut conn, &svc_paper::PaperRef::source("arxiv:B".into())).unwrap();

        let latest = paper_ref(&conn, "arxiv:A", None).unwrap();
        assert_eq!((latest.source_fk, latest.version), (a, 2));
        assert_eq!(latest.r#ref, format!("linxiv://paper/{a}?v=2"));
        assert_eq!(paper_ref(&conn, "arxiv:A", Some(1)).unwrap().version, 1);
        assert!(paper_ref(&conn, "arxiv:A", Some(9)).is_err());
        assert!(paper_ref(&conn, "arxiv:B", None).is_err(), "trashed");

        let refs: Vec<String> = vec![
            PaperRef {
                source_fk: a,
                version: 2,
            }
            .to_string(),
            PaperRef {
                source_fk: a,
                version: 1,
            }
            .to_string(),
            PaperRef {
                source_fk: a,
                version: 9,
            }
            .to_string(),
            PaperRef {
                source_fk: a + b + 100,
                version: 1,
            }
            .to_string(),
            PaperRef {
                source_fk: b,
                version: 1,
            }
            .to_string(),
            "nonsense".into(),
        ];
        let out = resolve_refs(&conn, &refs).unwrap();
        let got: Vec<_> = out
            .iter()
            .map(|r| {
                (
                    r.status,
                    r.source_id.as_deref(),
                    r.title.is_some(),
                    r.latest_version,
                )
            })
            .collect();
        assert_eq!(
            got,
            vec![
                (RefStatus::Current, Some("arxiv:A"), true, Some(2)),
                (RefStatus::Stale, Some("arxiv:A"), true, Some(2)),
                (RefStatus::Unknown, Some("arxiv:A"), false, Some(2)), // version never stored
                (RefStatus::Unknown, None, false, None),               // no such root
                (RefStatus::Unknown, None, false, None),               // trashed
                (RefStatus::Malformed, None, false, None),
            ]
        );
        assert_eq!(out[0].r#ref, refs[0]);
        assert_eq!(
            serde_json::to_string(&out[5]).unwrap(),
            r#"{"ref":"nonsense","status":"malformed","source_id":null,"title":null,"latest_version":null}"#
        );
    }
}
