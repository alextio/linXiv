//! Surface parity: every route arm must be reachable from the CLI and MCP, every
//! CLI leaf and MCP tool must mirror a route, unless `goldens/parity_exclusions.tsv`
//! names the gap with a reason. Links are derived from `METHOD /api/path` citations
//! in each CLI variant's comment block and each MCP tool's comment block, never
//! hand-paired. `goldens/parity.tsv` is the generated matrix (`PARITY_REGEN=1`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

const METHODS: [&str; 5] = ["GET", "POST", "PUT", "PATCH", "DELETE"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn rs_files(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|p| {
            let t = std::fs::read_to_string(&p).unwrap();
            (p, t)
        })
        .collect()
}

/// `("GET", ["api", "papers", id])` → `GET /api/papers/{}`; `rest @ ..` → `...`.
fn route_arms() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (_, src) in rs_files(&repo_root().join("src-tauri/crates/server/src/route")) {
        for (i, _) in src.match_indices("(\"") {
            let rest = &src[i + 2..];
            let Some(q) = rest.find('"') else { continue };
            let method = &rest[..q];
            if !METHODS.contains(&method) {
                continue;
            }
            let after = rest[q + 1..].trim_start_matches([',', ' ']);
            let Some(body) = after.strip_prefix('[') else {
                continue;
            };
            let Some(close) = body.find(']') else {
                continue;
            };
            let tail = body[close + 1..].trim_start_matches([')', ' ']);
            if !tail.starts_with("=>") && !tail.starts_with("if ") {
                continue;
            }
            let segs: Vec<String> = body[..close]
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| match s.strip_prefix('"') {
                    Some(lit) => lit.trim_end_matches('"').to_string(),
                    None if s.ends_with("..") => "...".to_string(),
                    None => "{}".to_string(),
                })
                .collect();
            out.insert(format!("{method} /{}", segs.join("/")));
        }
    }
    out
}

/// Every `METHOD /api/...` mention in `text`, `{name}` normalized to `{}`.
fn citations(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (i, _) in text.match_indices(" /api/") {
        let head = &text[..i];
        let Some(method) = METHODS.iter().find(|m| head.ends_with(*m)) else {
            continue;
        };
        let path: String = text[i + 1..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || "/{}-_.*".contains(*c))
            .collect();
        // A citation ends a sentence (`...`.), so strip one trailing dot only.
        let path = path
            .strip_suffix('.')
            .filter(|p| !p.ends_with(".."))
            .unwrap_or(&path);
        let mut norm = String::new();
        let mut in_brace = false;
        for c in path.chars() {
            match c {
                '{' => {
                    in_brace = true;
                    norm.push('{');
                }
                '}' => {
                    in_brace = false;
                    norm.push('}');
                }
                _ if in_brace => {}
                _ => norm.push(c),
            }
        }
        out.insert(format!("{method} {norm}"));
    }
    out
}

/// MCP tool name → routes cited in the comment block between the previous method
/// and this `#[tool]` fn.
fn mcp_tools() -> BTreeMap<String, BTreeSet<String>> {
    let mut out = BTreeMap::new();
    for (_, src) in rs_files(&repo_root().join("src-tauri/crates/mcp/src")) {
        for (i, _) in src.match_indices("#[tool") {
            if src[i..].starts_with("#[tool_router") {
                continue;
            }
            let rest = &src[i..];
            let Some(f) = rest.find("pub async fn ") else {
                continue;
            };
            let name: String = rest[f + 13..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let block_start = src[..i].rfind("\n    }\n").map_or(0, |j| j + 7);
            out.insert(name, citations(&src[block_start..i + f]));
        }
    }
    out
}

fn help(argv: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_linxiv-cli"))
        .args(argv)
        .arg("--help")
        .env(
            "LINXIV_DATA_DIR",
            std::env::temp_dir().join("linxiv-parity"),
        )
        .output()
        .expect("run linxiv-cli");
    String::from_utf8(out.stdout).unwrap()
}

fn subcommands(help: &str) -> Vec<String> {
    help.lines()
        .skip_while(|l| l.trim() != "Commands:")
        .skip(1)
        .take_while(|l| !l.trim().is_empty())
        .filter_map(|l| l.strip_prefix("  ").filter(|r| !r.starts_with(' ')))
        .filter_map(|r| r.split_whitespace().next())
        .filter(|n| *n != "help")
        .map(str::to_string)
        .collect()
}

/// Leaf argv (space-joined) → routes cited above its clap variant.
fn cli_leaves() -> BTreeMap<String, BTreeSet<String>> {
    fn walk(argv: Vec<String>, out: &mut Vec<Vec<String>>) {
        let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        let subs = subcommands(&help(&refs));
        if subs.is_empty() {
            out.push(argv);
            return;
        }
        for s in subs {
            let mut next = argv.clone();
            next.push(s);
            walk(next, out);
        }
    }
    let mut leaves = Vec::new();
    walk(vec![], &mut leaves);

    let mut sources = rs_files(&repo_root().join("src-tauri/crates/cli/src/cmd"));
    sources.push((
        repo_root().join("src-tauri/crates/cli/src/main.rs"),
        std::fs::read_to_string(repo_root().join("src-tauri/crates/cli/src/main.rs")).unwrap(),
    ));
    let all: String = sources
        .iter()
        .map(|(_, t)| t.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    leaves
        .into_iter()
        .map(|argv| {
            let leaf = argv.join(" ");
            let enum_name = match argv.len() {
                1 => "Commands".to_string(),
                2 if argv[0] == "settings" => "SettingsCmd".to_string(),
                2 => format!("{}Cmd", pascal(&argv[0])),
                _ => format!("{}Cmd", pascal(&argv[argv.len() - 2])),
            };
            let variant = pascal(&argv[argv.len() - 1]);
            let body = enum_body(&all, &enum_name)
                .unwrap_or_else(|| panic!("enum {enum_name} for `{leaf}` not found"));
            let cited = variant_comment(body, &variant)
                .map(citations)
                .unwrap_or_else(|| panic!("variant {enum_name}::{variant} for `{leaf}` not found"));
            (leaf, cited)
        })
        .collect()
}

fn pascal(s: &str) -> String {
    s.split('-')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_ascii_uppercase())
                .into_iter()
                .chain(c)
                .collect::<String>()
        })
        .collect()
}

fn enum_body<'a>(src: &'a str, name: &str) -> Option<&'a str> {
    let i = src.find(&format!("enum {name} {{"))?;
    let end = src[i..].find("\n}\n")?;
    Some(&src[i..i + end])
}

/// The comment block directly above `    Variant` (4-space indent, then `{`, `(`, `,` or eol).
fn variant_comment<'a>(body: &'a str, variant: &str) -> Option<&'a str> {
    let lines: Vec<&str> = body.lines().collect();
    let idx = lines.iter().position(|l| {
        l.strip_prefix("    ")
            .and_then(|r| r.strip_prefix(variant))
            .is_some_and(|r| r.is_empty() || r.starts_with([' ', '{', '(', ',']))
    })?;
    let mut j = idx;
    while j > 0 && (lines[j - 1].trim().starts_with("//") || lines[j - 1].trim().starts_with("#["))
    {
        j -= 1;
    }
    let start = lines[..j].iter().map(|l| l.len() + 1).sum::<usize>();
    let end = lines[..idx].iter().map(|l| l.len() + 1).sum::<usize>();
    Some(&body[start..end])
}

/// (surface, name) → set of surfaces it is deliberately missing on.
fn exclusions() -> BTreeMap<(String, String), BTreeSet<String>> {
    std::fs::read_to_string(repo_root().join("goldens/parity_exclusions.tsv"))
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            assert!(
                c.len() >= 4 && !c[3].is_empty(),
                "exclusion needs a reason: {l}"
            );
            (
                (c[0].to_string(), c[1].to_string()),
                c[2].split(',').map(str::to_string).collect(),
            )
        })
        .collect()
}

#[test]
fn every_capability_is_on_all_three_surfaces_or_explicitly_excluded() {
    let routes = route_arms();
    let cli = cli_leaves();
    let mcp = mcp_tools();
    let excl = exclusions();
    let mut problems = Vec::new();

    let excluded = |surface: &str, name: &str, missing: &str| {
        excl.get(&(surface.to_string(), name.to_string()))
            .is_some_and(|m| m.contains(missing))
    };

    // Route → cited by at least one CLI leaf and one MCP tool.
    let mut matrix = Vec::new();
    for r in &routes {
        let on_cli: Vec<&String> = cli
            .iter()
            .filter(|(_, c)| c.contains(r))
            .map(|(n, _)| n)
            .collect();
        let on_mcp: Vec<&String> = mcp
            .iter()
            .filter(|(_, c)| c.contains(r))
            .map(|(n, _)| n)
            .collect();
        for (surface, hits) in [("cli", &on_cli), ("mcp", &on_mcp)] {
            if hits.is_empty() && !excluded("route", r, surface) {
                problems.push(format!(
                    "route `{r}` has no {surface} citation and no exclusion"
                ));
            }
            if !hits.is_empty() && excluded("route", r, surface) {
                problems.push(format!("route `{r}` is excluded on {surface} but {hits:?} cites it — drop the exclusion"));
            }
        }
        let cell = |v: &Vec<&String>| {
            if v.is_empty() {
                "-".to_string()
            } else {
                v.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(",")
            }
        };
        matrix.push(format!("{r}\t{}\t{}", cell(&on_cli), cell(&on_mcp)));
    }

    // CLI leaf / MCP tool → cites an existing route, or is excluded from `route`.
    for (surface, entries) in [("cli", &cli), ("mcp", &mcp)] {
        for (name, cited) in entries {
            for c in cited {
                if !routes.contains(c) {
                    problems.push(format!(
                        "{surface} `{name}` cites `{c}` which is not a route arm"
                    ));
                }
            }
            let ex = excluded(surface, name, "route");
            if cited.is_empty() && !ex {
                problems.push(format!(
                    "{surface} `{name}` cites no route and has no exclusion"
                ));
            }
            if !cited.is_empty() && ex {
                problems.push(format!("{surface} `{name}` is excluded from route but cites {cited:?} — drop the exclusion"));
            }
            if cited.is_empty() {
                matrix.push(match surface {
                    "cli" => format!("-\t{name}\t-"),
                    _ => format!("-\t-\t{name}"),
                });
            }
        }
    }

    // Exclusions must name something that exists.
    for (surface, name) in excl.keys() {
        let exists = match surface.as_str() {
            "route" => routes.contains(name),
            "cli" => cli.contains_key(name),
            "mcp" => mcp.contains_key(name),
            other => panic!("unknown surface `{other}` in parity_exclusions.tsv"),
        };
        if !exists {
            problems.push(format!(
                "exclusion for {surface} `{name}` is stale: it no longer exists"
            ));
        }
    }

    let generated = format!(
        "# GENERATED by crates/cli/tests/parity.rs (PARITY_REGEN=1 to refresh). DO NOT EDIT.\n# route\tcli\tmcp — `-` is a gap justified in parity_exclusions.tsv.\n{}\n",
        matrix.join("\n")
    );
    let path = repo_root().join("goldens/parity.tsv");
    if std::env::var_os("PARITY_REGEN").is_some() {
        std::fs::write(&path, &generated).unwrap();
    } else if std::fs::read_to_string(&path).ok().as_deref() != Some(generated.as_str()) {
        problems.push("goldens/parity.tsv is stale — run with PARITY_REGEN=1".to_string());
    }

    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}
