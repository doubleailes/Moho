//! The display style override filter
//! (`HdsiLegacyDisplayStyleOverrideSceneIndex`).
#![allow(clippy::unwrap_used)]

mod common;

use std::rc::Rc;

use common::{p, stage};
use moho::chain;
use moho::data_source::locate;
use moho::data_source::retained::{RetainedContainer, container, value};
use moho::filter::LegacyDisplayStyleOverrideSceneIndex;
use moho::locator::{Locator, LocatorSet};
use moho::openusd::sdf;
use moho::openusd::tf::Token;
use moho::openusd::vt::Value;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{Added, Dirtied, Prim, RetainedSceneIndex, SceneIndex};
use moho::stage::StageSceneIndex;

fn read(index: &dyn SceneIndex, path: &str, locator: &str) -> Option<Value> {
    let data = index.prim(&p(path)).data?;
    locate(&data, &Locator::parse(locator)).and_then(|d| d.value())
}

/// `/a` with no display style, `/a/b` with its own refine level and cull
/// style.
fn retained() -> Rc<RetainedSceneIndex> {
    let input = RetainedSceneIndex::new();
    input.add_prims([
        (
            p("/a"),
            Prim::new(
                "mesh",
                RetainedContainer::new([("mesh", container(Vec::<(&str, _)>::new()))]).into_handle(),
            ),
        ),
        (
            p("/a/b"),
            Prim::new(
                "mesh",
                RetainedContainer::new([(
                    "displayStyle",
                    container([
                        ("refineLevel", value(1)),
                        ("cullStyle", value(Token::from("front"))),
                    ]),
                )])
                .into_handle(),
            ),
        ),
    ]);
    input
}

fn dirtied(path: &str, locator: &str) -> Notice {
    Notice::Dirtied(Dirtied {
        path: if path == "/" {
            sdf::Path::abs_root()
        } else {
            p(path)
        },
        locators: LocatorSet::from(Locator::parse(locator)),
    })
}

#[test]
fn no_fallback_answers_nothing_but_names_both() {
    let index = LegacyDisplayStyleOverrideSceneIndex::new(retained());
    assert_eq!(read(index.as_ref(), "/a", "displayStyle/refineLevel"), None);
    assert_eq!(read(index.as_ref(), "/a", "displayStyle/cullStyle"), None);
    let data = index.prim(&p("/a")).data.unwrap();
    let mut names = locate(&data, &Locator::parse("displayStyle"))
        .unwrap()
        .as_container()
        .unwrap()
        .names();
    names.sort();
    assert_eq!(names, ["cullStyle", "refineLevel"]);
    // The prim's own data is kept.
    assert!(locate(&data, &Locator::parse("mesh")).is_some());
    // A prim with no data stays without.
    assert!(index.prim(&sdf::Path::abs_root()).data.is_none());
}

#[test]
fn fallbacks_fill_in_beneath_the_prims_own() {
    let index = LegacyDisplayStyleOverrideSceneIndex::new(retained());
    index.set_refine_level_fallback(Some(2));
    index.set_cull_style_fallback(Some(Token::from("back")));
    assert_eq!(index.refine_level_fallback(), Some(2));
    assert_eq!(index.cull_style_fallback(), Some(Token::from("back")));

    assert_eq!(
        read(index.as_ref(), "/a", "displayStyle/refineLevel"),
        Some(Value::Int(2))
    );
    assert_eq!(
        read(index.as_ref(), "/a", "displayStyle/cullStyle"),
        Some(Value::Token(Token::from("back")))
    );
    // The prim's own opinion wins.
    assert_eq!(
        read(index.as_ref(), "/a/b", "displayStyle/refineLevel"),
        Some(Value::Int(1))
    );
    assert_eq!(
        read(index.as_ref(), "/a/b", "displayStyle/cullStyle"),
        Some(Value::Token(Token::from("front")))
    );

    // Unset again: nothing.
    index.set_refine_level_fallback(None);
    index.set_cull_style_fallback(Some(Token::from("")));
    assert_eq!(index.cull_style_fallback(), None);
    assert_eq!(read(index.as_ref(), "/a", "displayStyle/refineLevel"), None);
    assert_eq!(read(index.as_ref(), "/a", "displayStyle/cullStyle"), None);
}

#[test]
fn prims_read_before_a_change_see_it() {
    let index = LegacyDisplayStyleOverrideSceneIndex::new(retained());
    let data = index.prim(&p("/a")).data.unwrap();
    index.set_refine_level_fallback(Some(3));
    assert_eq!(
        locate(&data, &Locator::parse("displayStyle/refineLevel")).and_then(|d| d.value()),
        Some(Value::Int(3))
    );
}

#[test]
fn a_new_fallback_dirties_every_prim() {
    let input = retained();
    let index = LegacyDisplayStyleOverrideSceneIndex::new(input.clone());
    let recorder = RecordingObserver::on(index.as_ref());

    index.set_refine_level_fallback(Some(1));
    assert_eq!(
        recorder.take(),
        [
            dirtied("/", "displayStyle/refineLevel"),
            dirtied("/a", "displayStyle/refineLevel"),
            dirtied("/a/b", "displayStyle/refineLevel"),
        ]
    );
    // The same value: nothing.
    index.set_refine_level_fallback(Some(1));
    assert_eq!(recorder.take(), []);
    index.set_refine_level_fallback(None);
    assert_eq!(recorder.take().len(), 3);

    // The cull style dirties all of `displayStyle`, as Hydra does.
    index.set_cull_style_fallback(Some(Token::from("back")));
    assert_eq!(
        recorder.take(),
        [
            dirtied("/", "displayStyle"),
            dirtied("/a", "displayStyle"),
            dirtied("/a/b", "displayStyle"),
        ]
    );
    index.set_cull_style_fallback(Some(Token::from("back")));
    assert_eq!(recorder.take(), []);
    // `None` and the empty token are the same unset fallback.
    index.set_cull_style_fallback(None);
    assert_eq!(recorder.take().len(), 3);
    index.set_cull_style_fallback(Some(Token::from("")));
    assert_eq!(recorder.take(), []);

    // The input's notices pass through.
    input.add_prims([(
        p("/c"),
        Prim::new(
            "mesh",
            RetainedContainer::new(Vec::<(&str, _)>::new()).into_handle(),
        ),
    )]);
    assert_eq!(
        recorder.take(),
        [Notice::Added(Added {
            path: p("/c"),
            ty: Token::from("mesh"),
        })]
    );
}

#[test]
fn over_the_standard_chain() {
    let (_dir, stage) = stage(
        r#"#usda 1.0
def Xform "world"
{
    def Mesh "geo"
    {
        int[] faceVertexCounts = [4]
        int[] faceVertexIndices = [0, 1, 2, 3]
        point3f[] points = [(0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0)]
        uniform token subdivisionScheme = "catmullClark"
    }
}
"#,
    );
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(stage);
    let index = LegacyDisplayStyleOverrideSceneIndex::new(chain::standard(stage_index.clone()));
    let recorder = RecordingObserver::on(index.as_ref());

    // `usdrecord --complexity high`.
    let level = moho::filter::legacy_display_style_override::refine_level_for_complexity(1.2);
    index.set_refine_level_fallback(Some(level));
    assert_eq!(
        read(index.as_ref(), "/world/geo", "displayStyle/refineLevel"),
        Some(Value::Int(2))
    );
    let notices = recorder.take();
    assert!(notices.contains(&dirtied("/world/geo", "displayStyle/refineLevel")));
    assert!(notices.contains(&dirtied("/", "displayStyle/refineLevel")));
}
