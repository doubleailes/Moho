#![allow(clippy::unwrap_used)]

//! The notice probe's observed behavior at openusd `26c81d8`, recorded in
//! `docs/openusd-notes.md` ("Notices across loading"). A bump that changes
//! it fails here.

use moho_spike::fixture::{self, TempDir};
use moho_spike::probe::{self, EDITED_ATTRIBUTE, Notice};

fn commit(resynced: &[&str], changed_info_only: &[&str]) -> Notice {
    Notice::Commit {
        resynced: resynced.iter().map(|s| s.to_string()).collect(),
        changed_info_only: changed_info_only.iter().map(|s| s.to_string()).collect(),
    }
}

fn load_rules(subtrees: &[&str], prims: &[&str]) -> Notice {
    Notice::LoadRules {
        subtrees: subtrees.iter().map(|s| s.to_string()).collect(),
        prims: prims.iter().map(|s| s.to_string()).collect(),
    }
}

#[test]
fn notices_across_loading() {
    let dir = TempDir::new().unwrap();
    let root = fixture::write(dir.path()).unwrap();
    let steps = probe::run(root.to_str().unwrap()).unwrap();
    for step in &steps {
        println!("{step}");
    }

    let prim = probe::edited_prim();
    let attribute = format!("{prim}.{EDITED_ATTRIBUTE}");
    let names: Vec<_> = steps.iter().map(|s| s.name).collect();
    assert_eq!(
        names,
        [
            "override unloaded prim",
            "author under unloaded payload",
            "load payload",
            "edit loaded",
            "unload payload",
            "edit after unload",
        ]
    );
    assert!(steps.iter().all(|s| s.error.is_none()), "{steps:?}");

    // An edit under an unloaded payload notifies, as an edit, in stage
    // namespace: a new spec resyncs, a new value is info-only.
    assert_eq!(steps[0].notices, [commit(&[&prim], &[])]);
    assert_eq!(
        steps[1].notices,
        [commit(&[&attribute], &[]), commit(&[], &[&attribute])]
    );
    assert_eq!(steps[1].attribute, Some(1.0));

    // Loading and unloading notify only through `load_rules_changed`, never
    // as `after_commit`, so an index can tell them from edits. The loaded
    // prim is the subtree resync, and its ancestor whose loaded state
    // flipped with no payload of its own (`/world` under `LoadNone`) a prim
    // recomposed in place, so sibling chunks are not covered
    // (mxpv/openusd#174).
    let element = fixture::element_path(0);
    assert_eq!(steps[2].notices, [load_rules(&[&element], &["/world"])]);
    assert_eq!(steps[4].notices, [load_rules(&[&element], &["/world"])]);

    // Edits while loaded, and after unloading again, are info-only.
    assert_eq!(steps[3].notices, [commit(&[], &[&attribute])]);
    assert_eq!(steps[5].notices, [commit(&[], &[&attribute])]);
    assert_eq!(steps[5].attribute, Some(3.0));

    // The root layer's `over` composes the prim even while its payload is
    // unloaded, so the authored value stays readable after the unload.
    assert_eq!(steps[4].attribute, Some(2.0));
}
