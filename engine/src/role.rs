//! Role short/long form — P0.2, `proposals/atom-grammar/README.md` §3, in
//! `cic-primitives` (`CentralInfraCore/cic-primitives`).
//!
//! A direct, deliberately narrow port of that repo's own reference
//! implementation, `check_grammar.py`'s `expand_role`/`surface_default_authority`
//! — run directly against the real Python source in this session to ground
//! every branch and every edge case, not inferred from reading it. This is
//! `Stage::Normalize`'s job ("short forms expanded... applicable defaults
//! applied"), ported here as its own small module rather than folded into
//! a general-purpose "normalize everything" module that does not exist yet
//! and should not be implied by this one piece of it.
//!
//! # Why this, and not Access, is the first Normalize-stage piece
//!
//! Access's own short/long form (`access.yaml`'s `key: value` vs.
//! `key: {value, access, modify, inherit, default_injection, conformance}`)
//! was the originally-planned next step, but `cic-primitives`' own grammar
//! effort (`proposals/atom-grammar/README.md`) explicitly scopes it out as
//! P0.4, not yet decided — the only written description is in the archived,
//! rejected `cic-object-model`'s SPEC §6.4, which that same document names
//! as carrying an unresolved defect (`inherit`'s three states, `true`/
//! `false`/`0`, are not a stable type as a boolean). Implementing against
//! an upstream grammar that is explicitly still open would mean either
//! importing that known defect as a decision nobody here actually made, or
//! inventing new semantics in a consumer repo — this engine implements
//! semantics, it does not invent them. Raised as
//! `CentralInfraCore/cic-primitives#17`, not worked around here.
//!
//! Role, by contrast, is P0.2 — closed, enforced by `check_grammar.py`,
//! measured against a real corpus (`kubernetes-pod.yaml`,
//! `compute-resource.yaml`: 32 occurrences). A decided contract to
//! implement, not one to guess at. Ported against that *executable*
//! reference specifically, not `atom-grammar/README.md`'s own prose — which
//! a review on this module caught still drifting from it in one place
//! (`key`'s authority; see [`surface_default_authority`]'s own doc comment),
//! fixed upstream in `cic-primitives#18`, not worked around here.
//!
//! # What this module does not do
//!
//! `expand_role` in the Python reference returns an *unvalidated* long
//! form: a dict long-form's `authority`/`lifecycle` strings are not checked
//! against the legal value set at this point — that is
//! `check_role_algebra`/`check_role_against_surface`'s job, a separate,
//! later concern (`Validate`, not `Normalize`). This module mirrors that
//! split faithfully: [`Role`]'s fields are plain strings, not a Rust enum
//! that would reject an illegal value this engine has not yet been asked to
//! validate — tightening that here would be this engine inventing a
//! stricter contract than `cic-primitives` itself enforces at this stage,
//! not implementing the one that exists.

use crate::error::{code, Error, Result, Stage};
use crate::reader::Value;

/// Role's long form. Mirrors `expand_role`'s own return shape exactly:
/// `authority`/`lifecycle` are carried as the raw strings the input gave
/// (or a derived default), not validated against the legal value set here
/// — see the module's own doc comment for why.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Role {
    pub authority: String,
    pub structural: Vec<String>,
    pub lifecycle: Option<String>,
}

/// One `SHORT_ROLE_EXPANSION` table entry: the short-form string, and the
/// long form it expands to (an absent `authority` resolves from the
/// surface, not a fixed value — only `key`'s does).
struct ShortRoleExpansion {
    name: &'static str,
    authority: Option<&'static str>,
    structural: &'static [&'static str],
    lifecycle: Option<&'static str>,
}

/// Short-form strings and the long form each expands to, mirroring
/// `check_grammar.py`'s `SHORT_ROLE_EXPANSION` exactly — including which
/// facts it leaves out: `key`'s `authority` is `None` here (resolved from
/// the surface, not fixed), and `derived`/`volatile` both expand to
/// `authority: state` with the lifecycle set, not a separate authority of
/// their own.
const SHORT_ROLE_EXPANSION: &[ShortRoleExpansion] = &[
    ShortRoleExpansion {
        name: "config",
        authority: Some("config"),
        structural: &[],
        lifecycle: None,
    },
    ShortRoleExpansion {
        name: "state",
        authority: Some("state"),
        structural: &[],
        lifecycle: None,
    },
    ShortRoleExpansion {
        name: "operational",
        authority: Some("operational"),
        structural: &[],
        lifecycle: None,
    },
    ShortRoleExpansion {
        name: "key",
        authority: None,
        structural: &["key"],
        lifecycle: None,
    },
    ShortRoleExpansion {
        name: "derived",
        authority: Some("state"),
        structural: &[],
        lifecycle: Some("derived"),
    },
    ShortRoleExpansion {
        name: "volatile",
        authority: Some("state"),
        structural: &[],
        lifecycle: Some("volatile"),
    },
];

/// Which authorities a surface admits, and which one a node inherits when
/// it declares no role at all — mirrors `SURFACE_AUTHORITY` and
/// `surface_default_authority` exactly, including the easy-to-get-wrong
/// detail verified by actually running the Python in this session, not
/// assumed from reading it: `surface_default_authority("state_surface")`
/// is `"operational"`, not `"state"` — `sorted({"state", "operational"})`
/// puts `"operational"` first (`'o' < 's'`), and the Python takes
/// `sorted(...)[0]`.
///
/// This is also `key`'s own authority once expanded (`SHORT_ROLE_EXPANSION`'s
/// `"key"` entry has `authority: None`) — ported against the *executable*
/// reference, deliberately, not `atom-grammar/README.md`'s own prose, which
/// a review on this PR caught still drifted from it: the README said `key`'s
/// authority is always `config`, while `check_grammar.py`'s own comments
/// explain a later, corpus-measured correction making it surface-derived
/// (`container_statuses[].name` is a key the adapter *reports*, on a
/// `state_surface`, not one the requester supplies). Fixed upstream in
/// `cic-primitives#18`, not worked around here — this module's own behavior
/// was already correct before that PR, since it was ported from the code,
/// not the prose.
fn surface_default_authority(surface: Option<&str>) -> &'static str {
    match surface {
        Some("config_surface") => "config",
        Some("state_surface") => "operational",
        _ => "config",
    }
}

/// Resolves `raw` (the raw, un-normalized value of a node's `role` member)
/// into [`Role`]'s long form, given the enclosing surface (`"config_surface"`/
/// `"state_surface"`/`None`, exactly as the Python's own `surface` parameter
/// is threaded through the node walk by that key's own name).
///
/// A missing `role` (`raw: None`) is itself a valid, empty role — not an
/// error — mirroring the Python's own `expand_role(None, ...)` exactly;
/// `check_address_key_fields`-equivalent logic (making a *missing* role an
/// error in a specific structural position) is a separate, not-yet-ported
/// concern.
///
/// # Errors
/// Returns `E_INVALID_ROLE` (rule `R-SHORT`, the Python reference's own
/// rule name, kept so the two can be cross-checked directly) for the three
/// cases the reference rejects: `role: reference` (no short form exists —
/// a reference's authority cannot be derived), an unrecognized short-form
/// string, or a `role` that is neither a string, a mapping, nor absent.
pub fn expand_role(raw: Option<&Value>, path: &str, surface: Option<&str>) -> Result<Role> {
    match raw {
        None => Ok(Role {
            authority: surface_default_authority(surface).to_string(),
            structural: Vec::new(),
            lifecycle: None,
        }),
        Some(Value::Str(s)) => expand_short_form(s, path, surface),
        Some(Value::Map(m)) => Ok(Role {
            authority: m
                .get("authority")
                .and_then(Value::as_str)
                .unwrap_or("config")
                .to_string(),
            structural: m
                .get("structural")
                .and_then(Value::as_seq)
                .map(|seq| {
                    seq.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            lifecycle: m
                .get("lifecycle")
                .and_then(Value::as_str)
                .map(str::to_string),
        }),
        Some(_) => Err(invalid_role(
            path,
            "`role` must be a string or a mapping".to_string(),
        )),
    }
}

fn expand_short_form(s: &str, path: &str, surface: Option<&str>) -> Result<Role> {
    if s == "reference" {
        return Err(invalid_role(
            path,
            "`role: reference` has no short form: a reference's authority cannot be \
             derived. Write the long form with an explicit authority."
                .to_string(),
        ));
    }
    let Some(entry) = SHORT_ROLE_EXPANSION.iter().find(|entry| entry.name == s) else {
        let known: Vec<&str> = {
            let mut names: Vec<&str> = SHORT_ROLE_EXPANSION
                .iter()
                .map(|entry| entry.name)
                .collect();
            names.sort_unstable();
            names
        };
        return Err(invalid_role(
            path,
            format!("`{s}` is not a short role form ({})", known.join(", ")),
        ));
    };
    Ok(Role {
        authority: entry
            .authority
            .unwrap_or_else(|| surface_default_authority(surface))
            .to_string(),
        structural: entry.structural.iter().map(|s| (*s).to_string()).collect(),
        lifecycle: entry.lifecycle.map(str::to_string),
    })
}

fn invalid_role(path: &str, detail: String) -> Error {
    Error::new(
        code::INVALID_ROLE,
        "R-SHORT",
        Stage::Normalize,
        path,
        detail,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::Map;

    fn long_map(pairs: &[(&str, Value)]) -> Value {
        let mut m = Map::default();
        for (k, v) in pairs {
            m.push(*k, v.clone());
        }
        Value::Map(m)
    }

    // Every case run directly against the real Python check_grammar.py in
    // this session (not inferred from reading it) -- re-verified here
    // against the Rust port, not re-trusting the port without a check.
    #[test]
    fn matches_python_reference_exactly() {
        assert_eq!(
            expand_role(None, "$", None).unwrap(),
            Role {
                authority: "config".into(),
                structural: vec![],
                lifecycle: None
            }
        );
        // The easy-to-get-wrong one: state_surface's default is
        // "operational", not "state" -- sorted({"state","operational"})[0].
        assert_eq!(
            expand_role(None, "$", Some("state_surface")).unwrap(),
            Role {
                authority: "operational".into(),
                structural: vec![],
                lifecycle: None
            }
        );
        assert_eq!(
            expand_role(Some(&Value::Str("key".into())), "$", None).unwrap(),
            Role {
                authority: "config".into(),
                structural: vec!["key".into()],
                lifecycle: None
            }
        );
        assert_eq!(
            expand_role(Some(&Value::Str("key".into())), "$", Some("state_surface")).unwrap(),
            Role {
                authority: "operational".into(),
                structural: vec!["key".into()],
                lifecycle: None
            }
        );
        assert_eq!(
            expand_role(Some(&Value::Str("derived".into())), "$", None).unwrap(),
            Role {
                authority: "state".into(),
                structural: vec![],
                lifecycle: Some("derived".into())
            }
        );
        assert_eq!(
            expand_role(Some(&Value::Str("volatile".into())), "$", None).unwrap(),
            Role {
                authority: "state".into(),
                structural: vec![],
                lifecycle: Some("volatile".into())
            }
        );
        assert_eq!(
            expand_role(Some(&long_map(&[])), "$", None).unwrap(),
            Role {
                authority: "config".into(),
                structural: vec![],
                lifecycle: None
            }
        );
        // The dict branch's own asymmetry, verified directly, not assumed:
        // it defaults authority to "config" regardless of surface -- even
        // under config_surface, let alone state_surface. Not "fixed" here.
        assert_eq!(
            expand_role(
                Some(&long_map(&[("authority", Value::Str("state".into()))])),
                "$",
                Some("config_surface")
            )
            .unwrap(),
            Role {
                authority: "state".into(),
                structural: vec![],
                lifecycle: None
            }
        );
    }

    #[test]
    fn reference_has_no_short_form() {
        let err = expand_role(Some(&Value::Str("reference".into())), "$", None).unwrap_err();
        assert_eq!(err.code, code::INVALID_ROLE);
        assert_eq!(err.rule, "R-SHORT");
        assert_eq!(err.stage, Stage::Normalize);
    }

    #[test]
    fn unknown_short_form_is_rejected() {
        let err = expand_role(Some(&Value::Str("bogus".into())), "$", None).unwrap_err();
        assert_eq!(err.code, code::INVALID_ROLE);
        assert_eq!(err.rule, "R-SHORT");
    }

    #[test]
    fn non_string_non_mapping_role_is_rejected() {
        let err = expand_role(Some(&Value::Int(42)), "$", None).unwrap_err();
        assert_eq!(err.code, code::INVALID_ROLE);
        assert_eq!(err.rule, "R-SHORT");
    }

    #[test]
    fn dict_long_form_passes_structural_through_unvalidated() {
        // Mirrors the Python's own unvalidated pass-through: this module
        // does not check that "not-a-real-value" is a legal structural
        // marker -- that is a separate, later concern.
        let role = expand_role(
            Some(&long_map(&[(
                "structural",
                Value::Seq(vec![Value::Str("not-a-real-value".into())]),
            )])),
            "$",
            None,
        )
        .unwrap();
        assert_eq!(role.structural, vec!["not-a-real-value".to_string()]);
    }
}
