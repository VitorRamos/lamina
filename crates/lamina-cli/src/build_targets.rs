//! Multi-target selection and tag pairing for `lamina build` / explain / emit.

/// One Buildx solve: Lamina target name + image tags to apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetSolve {
    /// Empty means "implicit default" (first IR target) — matches today's `lamina build`.
    pub target: String,
    pub tags: Vec<String>,
}

/// Resolve `--target` / `--all-targets` against published `pub target` names.
///
/// Empty `named` and `all == false` → `Ok([])` (caller keeps prior default:
/// explain/emit = all targets, build = first target).
pub fn select_target_names(
    available: &[String],
    named: &[String],
    all: bool,
) -> Result<Vec<String>, String> {
    if all && !named.is_empty() {
        return Err("cannot combine --all-targets with --target".into());
    }
    if all {
        if available.is_empty() {
            return Err("no pub targets in project".into());
        }
        return Ok(available.to_vec());
    }
    if named.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity(named.len());
    for name in named {
        if !available.iter().any(|a| a == name) {
            return Err(format!(
                "unknown target `{name}` (available: {})",
                available.join(", ")
            ));
        }
        if !out.iter().any(|t| t == name) {
            out.push(name.clone());
        }
    }
    Ok(out)
}

/// Map selected targets + `--tag` flags to sequential solves.
///
/// * **0 or 1 target** (including implicit empty): all tags apply to that image;
///   default tag is `{package}:dev` when none given.
/// * **Multiple targets:** 0 tags → `{package}:{target}` each; N tags for N
///   targets → pair in order; otherwise error.
pub fn plan_solves(
    package_name: &str,
    selected_targets: &[String],
    tags: &[String],
) -> Result<Vec<TargetSolve>, String> {
    if selected_targets.len() > 1 {
        let mapped: Vec<String> = match tags.len() {
            0 => selected_targets
                .iter()
                .map(|t| format!("{package_name}:{t}"))
                .collect(),
            n if n == selected_targets.len() => tags.to_vec(),
            n => {
                return Err(format!(
                    "multi-target build needs 0 or {} --tag values (one per --target), got {n}",
                    selected_targets.len()
                ));
            }
        };
        return Ok(selected_targets
            .iter()
            .zip(mapped)
            .map(|(t, tag)| TargetSolve {
                target: t.clone(),
                tags: vec![tag],
            })
            .collect());
    }

    let tags = if tags.is_empty() {
        vec![format!("{package_name}:dev")]
    } else {
        tags.to_vec()
    };
    Ok(vec![TargetSolve {
        target: selected_targets.first().cloned().unwrap_or_default(),
        tags,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn avail() -> Vec<String> {
        vec!["app".into(), "debug".into(), "sdk".into()]
    }

    #[test]
    fn implicit_empty_when_no_flags() {
        assert!(select_target_names(&avail(), &[], false)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn all_targets_lists_available() {
        assert_eq!(select_target_names(&avail(), &[], true).unwrap(), avail());
    }

    #[test]
    fn named_preserves_order_and_dedups() {
        let named = vec!["debug".into(), "app".into(), "debug".into()];
        assert_eq!(
            select_target_names(&avail(), &named, false).unwrap(),
            vec!["debug", "app"]
        );
    }

    #[test]
    fn rejects_unknown_and_combo() {
        assert!(select_target_names(&avail(), &["nope".into()], false).is_err());
        assert!(select_target_names(&avail(), &["app".into()], true).is_err());
    }

    #[test]
    fn single_target_keeps_all_tags() {
        let plan = plan_solves("pkg", &["app".into()], &["a:1".into(), "a:2".into()]).unwrap();
        assert_eq!(
            plan,
            vec![TargetSolve {
                target: "app".into(),
                tags: vec!["a:1".into(), "a:2".into()],
            }]
        );
    }

    #[test]
    fn implicit_single_defaults_dev_tag() {
        let plan = plan_solves("pkg", &[], &[]).unwrap();
        assert_eq!(
            plan,
            vec![TargetSolve {
                target: String::new(),
                tags: vec!["pkg:dev".into()],
            }]
        );
    }

    #[test]
    fn multi_default_tags_are_package_colon_target() {
        let plan = plan_solves("pkg", &["app".into(), "debug".into()], &[]).unwrap();
        assert_eq!(plan[0].tags, vec!["pkg:app".to_string()]);
        assert_eq!(plan[1].tags, vec!["pkg:debug".to_string()]);
        assert_eq!(plan[0].target, "app");
        assert_eq!(plan[1].target, "debug");
    }

    #[test]
    fn multi_pairs_tags_in_order() {
        let plan = plan_solves(
            "pkg",
            &["app".into(), "debug".into()],
            &["img:a".into(), "img:b".into()],
        )
        .unwrap();
        assert_eq!(plan[0].tags, vec!["img:a".to_string()]);
        assert_eq!(plan[1].tags, vec!["img:b".to_string()]);
    }

    #[test]
    fn multi_rejects_mismatched_tag_count() {
        assert!(plan_solves("pkg", &["a".into(), "b".into()], &["only-one".into()]).is_err());
    }
}
