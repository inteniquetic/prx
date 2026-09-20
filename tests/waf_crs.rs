//! The whole of OWASP CRS v4.21.0 through the SecLang loader (T505).
//!
//! CRS is not vendored: `bench/waf/fetch-crs.sh` clones the pinned release
//! into the git-ignored `bench/waf/crs/`. Without it this test skips and says
//! so; with `PRX_REQUIRE_CRS=1` (CI sets it) a missing checkout is a failure,
//! so the numbers below cannot quietly stop being checked.

use std::path::{Path, PathBuf};
use std::time::Instant;

use prx::waf::parse::parse;
use prx::waf::rule::{Action, Directive, DirectiveKind};
use prx::waf::{Loaded, load};

fn crs() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("bench/waf/crs");
    if root.join("rules").is_dir() {
        return Some(root);
    }
    assert!(
        std::env::var_os("PRX_REQUIRE_CRS").is_none(),
        "PRX_REQUIRE_CRS is set but {} is missing; run bench/waf/fetch-crs.sh",
        root.display()
    );
    println!(
        "SKIPPED: no CRS checkout at {}; run bench/waf/fetch-crs.sh",
        root.display()
    );
    None
}

/// Every rule whose pattern is not used as written, by id. A repaired pattern
/// is a reviewed exception, so the list is exact: a CRS bump that adds one
/// fails here until somebody has looked at it.
///
/// The first seven are the ones T504 found (a literal `{`, or `\<` in a
/// class). The last six compile under `regex`'s Unicode mode, which is what
/// T504 measured, but not on bytes, which is what ModSecurity means: 934120
/// spells UTF-8 out as `\x{e2}\x91…`, and the 9424xx rules put `´’‘` in a class.
const REPAIRED: [u64; 13] = [
    921200, 932170, 932171, 933170, 933180, 934180, 941380, // T504
    934120, 942420, 942421, 942430, 942431, 942432,
];

/// The files T504 counted: the setup file, then the rules in CRS's own order.
fn load_crs(root: &Path) -> Loaded {
    load(&[root.join("crs-setup.conf"), root.join("rules/*.conf")])
}

#[test]
fn crs_loads_whole_and_matches_the_t504_census() {
    let Some(root) = crs() else { return };

    let started = Instant::now();
    let loaded = load_crs(&root);
    let elapsed = started.elapsed();
    println!("{loaded}");
    println!(
        "CRS load time: {elapsed:?} ({} build)",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    // A reload builds the new ruleset off to the side and swaps it in, so
    // this is time-to-effect, not time a request waits. The budget is 1 s in a
    // release build (~0.1 s measured). CI runs debug builds (~0.7 s measured),
    // so there the bound is loose: it exists to catch a 10x regression, not
    // to measure.
    let budget = if cfg!(debug_assertions) { 8.0 } else { 1.0 };
    assert!(
        elapsed.as_secs_f64() < budget,
        "CRS took {elapsed:?} to load"
    );

    assert_eq!(loaded.issues, [], "CRS must load without a single issue");
    let summary = &loaded.summary;
    let count = |name: &str| summary.directives.get(name).copied().unwrap_or(0);
    assert_eq!(
        (
            count("SecRule"),
            count("SecMarker"),
            count("SecAction"),
            count("SecDefaultAction"),
            count("SecComponentSignature"),
        ),
        (678, 30, 8, 2, 1),
        "T504 census (docs/decisions/0001-waf-engine.md)"
    );
    assert_eq!(
        summary.directives.values().sum::<usize>(),
        719,
        "and nothing else"
    );
    assert_eq!((summary.skipped, summary.removed), (0, 0));

    // All 312 @rx patterns compile, and the repair layer stays what it is
    // meant to be: a handful of PCRE-isms, not a rewrite of the ruleset.
    assert_eq!(summary.rx, 312);
    let mut repaired: Vec<u64> = loaded
        .ruleset
        .phases
        .iter()
        .flatten()
        .filter(|rule| {
            rule.links
                .iter()
                .any(|link| link.rx.as_ref().is_some_and(|rx| rx.repaired))
        })
        .map(|rule| rule.id)
        .collect();
    repaired.sort_unstable();
    let mut expected = REPAIRED.to_vec();
    expected.sort_unstable();
    println!("repaired: {repaired:?}");
    assert_eq!(
        repaired, expected,
        "the set of repaired rules changed: review each one"
    );
    assert_eq!(summary.rx_repaired, REPAIRED.len());

    // Every SecRule and SecAction is accounted for: as a rule, or as a link.
    let links: usize = loaded
        .ruleset
        .phases
        .iter()
        .flatten()
        .map(|rule| rule.links.len())
        .sum();
    assert_eq!(links, 678 + 8);
    assert_eq!(
        summary.rules,
        loaded.ruleset.phases.iter().map(Vec::len).sum::<usize>()
    );

    for (index, rules) in loaded.ruleset.phases.iter().enumerate() {
        for rule in rules {
            assert_eq!(usize::from(rule.phase), index + 1, "rule {}", rule.id);
            assert_ne!(rule.id, 0);

            // Chains: every link but the last says `chain`, and only the
            // head has an id.
            let (last, before) = rule.links.split_last().expect("a rule has a head");
            assert!(!last.actions.contains(&Action::Chain), "rule {}", rule.id);
            assert!(
                before
                    .iter()
                    .all(|link| link.actions.contains(&Action::Chain))
            );
            let has_id = |actions: &[Action]| actions.iter().any(|a| matches!(a, Action::Id(_)));
            assert!(rule.links[1..].iter().all(|link| !has_id(&link.actions)));

            // Every skipAfter found its marker, and lands inside the phase.
            let jumps = rule.links[0]
                .actions
                .iter()
                .any(|a| matches!(a, Action::SkipAfter(_)));
            assert_eq!(rule.skip_after.is_some(), jumps, "rule {}", rule.id);
            assert!(rule.skip_after.is_none_or(|target| target <= rules.len()));

            for link in &rule.links {
                assert!(link.transforms < loaded.ruleset.transform_chains.len());
                assert_eq!(link.rx.is_some(), link.operator.is_rx(), "rule {}", rule.id);
            }
        }
    }
    // 615 ids in CRS; the other 71 SecRule lines are links, one per `chain`.
    assert_eq!(summary.rules, 615);
    let chains = loaded
        .ruleset
        .phases
        .iter()
        .flatten()
        .filter(|rule| rule.links.len() > 1);
    assert_eq!(chains.count(), 54);

    // What T506 gets to skip: nothing in CRS reads these.
    let needs = &loaded.ruleset.needs;
    assert!(needs.contains("ARGS") && needs.contains("REQUEST_HEADERS") && needs.contains("XML"));
    assert!(!needs.contains("ARGS_POST_NAMES") && !needs.contains("GEO"));
    println!("needs: {needs:?}");
    println!(
        "distinct transformation chains: {}",
        loaded.ruleset.transform_chains.len()
    );
}

#[test]
fn every_crs_file_survives_a_round_trip() {
    let Some(root) = crs() else { return };
    let mut files = vec![root.join("crs-setup.conf")];
    let mut rules: Vec<PathBuf> = std::fs::read_dir(root.join("rules"))
        .expect("rules directory")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "conf"))
        .collect();
    rules.sort();
    files.extend(rules);

    let without_lines = |directives: Vec<Directive>| -> Vec<DirectiveKind> {
        let mut kinds: Vec<_> = directives
            .into_iter()
            .map(|directive| directive.kind)
            .collect();
        for kind in &mut kinds {
            if let DirectiveKind::Rule(links) = kind {
                links.iter_mut().for_each(|link| link.line = 0);
            }
        }
        kinds
    };
    for file in files {
        let name = file.display().to_string();
        let first = parse(&name, &std::fs::read_to_string(&file).expect("readable"));
        assert_eq!(first.issues, [], "{name}");
        let rendered: Vec<String> = first.directives.iter().map(Directive::to_string).collect();
        let second = parse(&name, &rendered.join("\n"));
        assert_eq!(second.issues, [], "{name}");
        assert_eq!(first.counts, second.counts, "{name}");
        assert!(
            without_lines(first.directives) == without_lines(second.directives),
            "{name}"
        );
    }
}
