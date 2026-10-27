#![allow(clippy::unwrap_used)]

mod common;

use std::rc::Rc;

use common::{TempDir, open, p, stage};
use moho::data_source::locate;
use moho::filter::{LightLinkingSceneIndex, NativeInstancingSceneIndex, PointInstancingSceneIndex};
use moho::locator::Locator;
use moho::openusd::gf;
use moho::openusd::sdf::{self, Value};
use moho::openusd::tf::Token;
use moho::scene_index::{CollectionMembership, SceneIndex};
use moho::stage::StageSceneIndex;

const PEBBLES: &str = r#"#usda 1.0
def Xform "world"
{
    def PointInstancer "scatter"
    {
        rel prototypes = [</world/scatter/protos/round>, </world/scatter/protos/flat>]
        int[] protoIndices = [0, 1, 0]
        point3f[] positions = [(0, 0, 0), (1, 0, 0), (2, 0, 0)]
        int64[] ids = [0, 1, 2]
        int64[] invisibleIds = [1]

        def Scope "protos"
        {
            def Xform "round"
            {
                double3 xformOp:translate = (0, 0.5, 0)
                uniform token[] xformOpOrder = ["xformOp:translate"]
                def Mesh "geo"
                {
                }
            }
            def Mesh "flat"
            {
            }
            def Mesh "unused"
            {
            }
        }
    }
}
"#;

fn read(index: &dyn SceneIndex, path: &sdf::Path, locator: &str) -> Option<Value> {
    locate(&index.prim(path).data?, &Locator::parse(locator)).and_then(|d| d.value())
}

fn indices(index: &dyn SceneIndex, path: &sdf::Path) -> Vec<Vec<i32>> {
    let data = index.prim(path).data.unwrap();
    let vector = locate(&data, &Locator::parse("instancerTopology/instanceIndices")).unwrap();
    let vector = vector.as_vector().unwrap().clone();
    (0..vector.len())
        .map(|i| match vector.get(i).unwrap().value() {
            Some(Value::IntVec(v)) => v,
            other => panic!("{other:?}"),
        })
        .collect()
}

#[test]
fn scattered_pebbles_and_invisible_ids() {
    let (_dir, usd) = stage(PEBBLES);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let scatter = p("/world/scatter");
    assert_eq!(stage_index.prim(&scatter).ty, "instancer");
    assert_eq!(
        read(stage_index.as_ref(), &scatter, "instancerTopology/prototypes"),
        Some(Value::PathVec(vec![
            p("/world/scatter/protos/round"),
            p("/world/scatter/protos/flat")
        ]))
    );
    assert_eq!(indices(stage_index.as_ref(), &scatter), [vec![0, 2], vec![1]]);
    assert_eq!(
        read(stage_index.as_ref(), &scatter, "instancerTopology/mask"),
        Some(Value::BoolVec(vec![true, false, true]))
    );
    let Some(Value::Vec3fVec(translations)) = read(
        stage_index.as_ref(),
        &scatter,
        "primvars/hydra:instanceTranslations/primvarValue",
    ) else {
        panic!()
    };
    assert_eq!(translations.len(), 3);
    assert_eq!(
        read(
            stage_index.as_ref(),
            &scatter,
            "primvars/hydra:instanceTranslations/interpolation"
        ),
        Some(Value::Token(Token::from("instance")))
    );
}

#[test]
fn prototypes_are_rerooted() {
    let (_dir, usd) = stage(PEBBLES);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let index = PointInstancingSceneIndex::new(stage_index);
    let scatter = p("/world/scatter");
    let Some(Value::PathVec(copies)) = read(index.as_ref(), &scatter, "instancerTopology/prototypes") else {
        panic!()
    };
    assert_eq!(copies.len(), 2);
    let round = &copies[0];
    assert!(
        round
            .as_str()
            .starts_with("/world/scatter/protos/round/ForInstancer"),
        "{round}"
    );
    assert!(index.children(&p("/world/scatter/protos/round")).contains(round));
    assert_eq!(
        read(index.as_ref(), round, "xform/resetXformStack"),
        Some(Value::Bool(true))
    );
    assert_eq!(
        read(index.as_ref(), round, "visibility/visibility"),
        Some(Value::Bool(true)),
        "the copy root is visible unless it says otherwise"
    );
    assert_eq!(
        round.as_str(),
        "/world/scatter/protos/round/ForInstancerad10eba5b2d19a4d",
        "named as UsdImaging (OpenUSD 26.08) names it"
    );
    assert_eq!(
        read(index.as_ref(), round, "xform/matrix"),
        Some(Value::Matrix4d(gf::Mat4d::translation([0.0, 0.5, 0.0]))),
        "the prototype's own transform is kept"
    );
    let geo = round.append_path("geo").unwrap();
    assert_eq!(index.prim(&geo).ty, "mesh");
    assert_eq!(
        read(index.as_ref(), &geo, "instancedBy/paths"),
        Some(Value::PathVec(vec![scatter.clone()]))
    );
    assert_eq!(
        index.prim(&p("/world/scatter/protos/round/geo")).ty,
        "",
        "the original is not drawn"
    );
    assert_eq!(
        index.prim(&p("/world/scatter/protos/unused")).ty,
        "",
        "untargeted prims are not drawn"
    );
}

#[test]
fn instance_categories_are_judged_per_instance() {
    let dir = TempDir::new().unwrap();
    dir.write("asset.usda", "#usda 1.0\n(\n    defaultPrim = \"asset\"\n)\ndef Xform \"asset\"\n{\n    def Mesh \"geo\"\n    {\n    }\n}\n");
    let root = dir.write(
        "root.usda",
        &format!(
            "{}\ndef SphereLight \"key\"\n{{\n    bool collection:lightLink:includeRoot = 0\n    rel collection:lightLink:includes = [</world/a>, </world/scatter>]\n}}\n",
            r#"#usda 1.0
def Xform "world"
{
    def Xform "a" (
        instanceable = true
        references = @./asset.usda@
    )
    {
    }
    def Xform "b" (
        instanceable = true
        references = @./asset.usda@
    )
    {
    }
    def PointInstancer "scatter"
    {
        rel prototypes = </world/scatter/proto>
        int[] protoIndices = [0, 0]
        point3f[] positions = [(0, 0, 0), (1, 0, 0)]
        def Mesh "proto"
        {
        }
    }
}"#
        ),
    );
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(open(&root));
    let native = NativeInstancingSceneIndex::new(stage_index.clone());
    let point = PointInstancingSceneIndex::new(native);
    let membership: Rc<dyn CollectionMembership> = stage_index.clone();
    let linking = LightLinkingSceneIndex::new(point, Some(membership));

    let category = match read(linking.as_ref(), &p("/key"), "light/lightLink") {
        Some(Value::Token(t)) => t,
        other => panic!("{other:?}"),
    };
    let per_instance = |instancer: &sdf::Path| -> Vec<bool> {
        let data = linking.prim(instancer).data.unwrap();
        let values = locate(&data, &Locator::parse("instanceCategories/categoriesValues")).unwrap();
        let values = values.as_vector().unwrap().clone();
        (0..values.len())
            .map(|i| {
                values
                    .get(i)
                    .unwrap()
                    .as_container()
                    .unwrap()
                    .names()
                    .contains(&category)
            })
            .collect()
    };
    let native_instancer = match read(linking.as_ref(), &p("/world/a"), "instance/instancer") {
        Some(Value::PathVec(mut v)) => v.pop().unwrap(),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        per_instance(&native_instancer),
        [true, false],
        "/world/a linked, /world/b not"
    );
    assert_eq!(
        per_instance(&p("/world/scatter")),
        [true, true],
        "judged at the point instancer's path"
    );
}

#[test]
fn prototype_copies_keep_their_origin() {
    let (_dir, usd) = stage(PEBBLES);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let index = PointInstancingSceneIndex::new(stage_index);
    let Some(Value::PathVec(copies)) = read(
        index.as_ref(),
        &p("/world/scatter"),
        "instancerTopology/prototypes",
    ) else {
        panic!()
    };
    let geo = copies[0].append_path("geo").unwrap();
    assert_eq!(
        read(index.as_ref(), &geo, "primOrigin/scenePath"),
        Some(Value::PathVec(vec![p("/world/scatter/protos/round/geo")])),
        "the original prototype prim"
    );
}

/// A point instancer inside another's prototype: every copy of the outer
/// prototype re-instances the inner instancer with copies of its own.
const NESTED: &str = r#"#usda 1.0
def Xform "world"
{
    def PointInstancer "outer"
    {
        rel prototypes = [</world/outer/protos/tree>]
        int[] protoIndices = [0, 0]
        point3f[] positions = [(0, 0, 0), (10, 0, 0)]
        def Scope "protos"
        {
            def Xform "tree"
            {
                def PointInstancer "leaves"
                {
                    rel prototypes = [</world/outer/protos/tree/leaves/protos/leaf>]
                    int[] protoIndices = [0, 0, 0]
                    point3f[] positions = [(0, 1, 0), (0, 2, 0), (0, 3, 0)]
                    def Scope "protos"
                    {
                        def Mesh "leaf"
                        {
                        }
                    }
                }
            }
        }
    }
}
"#;

#[test]
fn a_nested_instancer_is_reinstanced_in_every_copy() {
    let (_dir, usd) = stage(NESTED);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let index = PointInstancingSceneIndex::new(stage_index);
    // Names from `TfHash::Combine` of the instancer and the enclosing copy
    // root, as UsdImaging (OpenUSD 26.08) names them.
    let tree = p("/world/outer/protos/tree/ForInstancer5dde482a8a2c82ad");
    assert_eq!(
        read(index.as_ref(), &p("/world/outer"), "instancerTopology/prototypes"),
        Some(Value::PathVec(vec![tree.clone()]))
    );
    let leaves = tree.append_path("leaves").unwrap();
    assert_eq!(index.prim(&leaves).ty, "instancer", "the copied inner instancer");
    // The inner copies are made at the inner prototype's input path, named
    // for the enclosing copy, as UsdImaging makes them.
    let leaf = p("/world/outer/protos/tree/leaves/protos/leaf/ForInstancer1993e33b7f58b080");
    assert_eq!(
        read(index.as_ref(), &leaves, "instancerTopology/prototypes"),
        Some(Value::PathVec(vec![leaf.clone()]))
    );
    assert_eq!(index.prim(&leaf).ty, "mesh");
    assert!(index.children(&leaf.parent().unwrap()).contains(&leaf));
    assert_eq!(
        read(index.as_ref(), &leaf, "instancedBy/paths"),
        Some(Value::PathVec(vec![leaves.clone()]))
    );
    assert_eq!(
        read(index.as_ref(), &leaf, "instancedBy/prototypeRoots"),
        Some(Value::PathVec(vec![leaf.clone()]))
    );
    // The inner original inside the outer copy, and the original inner
    // instancer, are drawn only through their copies.
    assert_eq!(index.prim(&leaves.append_path("protos/leaf").unwrap()).ty, "");
    assert_eq!(index.prim(&p("/world/outer/protos/tree/leaves")).ty, "");
    assert_eq!(
        read(index.as_ref(), &leaf, "primOrigin/scenePath"),
        Some(Value::PathVec(vec![p(
            "/world/outer/protos/tree/leaves/protos/leaf"
        )]))
    );
}

const INACTIVE: &str = r#"#usda 1.0
def Xform "world"
{
    def PointInstancer "a" (
        inactiveIds = [2]
    )
    {
        rel prototypes = [</world/a/protos/round>, </world/a/protos/flat>]
        int[] protoIndices = [0, 1, 0]
        point3f[] positions = [(0, 0, 0), (1, 0, 0), (2, 0, 0)]
        int64[] ids = [0, 1, 2]
        int64[] invisibleIds = [1]
        def Scope "protos"
        {
            def Mesh "round"
            {
            }
            def Mesh "flat"
            {
            }
        }
    }
    def PointInstancer "b" (
        inactiveIds = [1, 3]
    )
    {
        rel prototypes = [</world/b/protos/round>, </world/b/protos/flat>]
        int[] protoIndices = [0, 1, 0, 1]
        point3f[] positions = [(0, 0, 0), (1, 0, 0), (2, 0, 0), (3, 0, 0)]
        def Scope "protos"
        {
            def Mesh "round"
            {
            }
            def Mesh "flat"
            {
            }
        }
    }
}
"#;

#[test]
fn inactive_ids_are_masked_not_dropped() {
    let (_dir, usd) = stage(INACTIVE);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let a = p("/world/a");
    assert_eq!(
        indices(stage_index.as_ref(), &a),
        [vec![0, 2], vec![1]],
        "the inactive instance 2 stays in the indices"
    );
    assert_eq!(
        read(stage_index.as_ref(), &a, "instancerTopology/mask"),
        Some(Value::BoolVec(vec![true, false, false])),
        "invisible 1 and inactive 2 are masked out"
    );
}

#[test]
fn a_fully_inactive_prototype_is_pruned() {
    let (_dir, usd) = stage(INACTIVE);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let b = p("/world/b");
    assert_eq!(
        read(stage_index.as_ref(), &b, "instancerTopology/prototypes"),
        Some(Value::PathVec(vec![p("/world/b/protos/round")]))
    );
    assert_eq!(indices(stage_index.as_ref(), &b), [vec![0, 2]]);
    assert_eq!(
        read(stage_index.as_ref(), &b, "instancerTopology/mask"),
        Some(Value::BoolVec(vec![true, false, true, false]))
    );
}

const STRAY_INDICES: &str = r#"#usda 1.0
def Xform "world"
{
    def PointInstancer "huge"
    {
        rel prototypes = [</world/huge/protos/round>, </world/huge/protos/flat>]
        int[] protoIndices = [0, 2000000000, 0]
        point3f[] positions = [(0, 0, 0), (1, 0, 0), (2, 0, 0)]
        def Scope "protos"
        {
            def Mesh "round"
            {
            }
            def Mesh "flat"
            {
            }
        }
    }
    def PointInstancer "pruned" (
        inactiveIds = [1]
    )
    {
        rel prototypes = [</world/pruned/protos/round>, </world/pruned/protos/flat>]
        int[] protoIndices = [0, 1, 3]
        point3f[] positions = [(0, 0, 0), (1, 0, 0), (2, 0, 0)]
        def Scope "protos"
        {
            def Mesh "round"
            {
            }
            def Mesh "flat"
            {
            }
        }
    }
}
"#;

#[test]
fn a_stray_prototype_index_reads_as_usd_imaging_without_allocating_it() {
    let (_dir, usd) = stage(STRAY_INDICES);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    // UsdImaging gives one array per index up to the largest one used,
    // here 2 000 000 001 of them, nearly all empty.
    let data = stage_index.prim(&p("/world/huge")).data.unwrap();
    let vector = locate(&data, &Locator::parse("instancerTopology/instanceIndices")).unwrap();
    let vector = vector.as_vector().unwrap().clone();
    assert_eq!(vector.len(), 2_000_000_001);
    let at = |i: usize| vector.get(i).and_then(|d| d.value());
    assert_eq!(at(0), Some(Value::IntVec(vec![0, 2])));
    assert_eq!(at(1), Some(Value::IntVec(vec![])));
    assert_eq!(at(1_999_999_999), Some(Value::IntVec(vec![])));
    assert_eq!(at(2_000_000_000), Some(Value::IntVec(vec![1])));
    assert_eq!(at(2_000_000_001), None);
    // Index 1 is fully inactive: pruned from both, the out-of-range index 3
    // kept after the unused 2, as UsdImaging.
    let pruned = p("/world/pruned");
    assert_eq!(
        read(stage_index.as_ref(), &pruned, "instancerTopology/prototypes"),
        Some(Value::PathVec(vec![p("/world/pruned/protos/round")]))
    );
    assert_eq!(indices(stage_index.as_ref(), &pruned), [vec![0], vec![], vec![2]]);
}

const PROXIES_AND_SCOPES: &str = r#"#usda 1.0
def Xform "staghorn"
{
    def Mesh "geo"
    {
        int[] faceVertexCounts = [3]
        int[] faceVertexIndices = [0, 1, 2]
        point3f[] points = [(0, 0, 0), (1, 0, 0), (0, 1, 0)]
    }
}
def Xform "world"
{
    def PointInstancer "coral"
    {
        rel prototypes = [</world/coral/Prototypes/staghorn/geo>]
        int[] protoIndices = [0, 0, 0]
        point3f[] positions = [(0, 0, 0), (2, 0, 0), (4, 0, 0)]
        def Scope "Prototypes"
        {
            def "staghorn" (
                instanceable = true
                prepend references = </staghorn>
            )
            {
            }
        }
    }
    def PointInstancer "rocks"
    {
        rel prototypes = [</world/rocks/Prototypes/rock>, </world/rocks/Prototypes/hidden>]
        int[] protoIndices = [0, 1]
        point3f[] positions = [(0, 0, 0), (2, 0, 0)]
        def Scope "Prototypes"
        {
            token visibility = "invisible"
            def Mesh "rock"
            {
            }
            def Mesh "hidden"
            {
                token visibility = "invisible"
            }
        }
    }
}
"#;

#[test]
fn a_proxy_target_copies_as_an_empty_prim() {
    let (_dir, usd) = stage(PROXIES_AND_SCOPES);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let index = PointInstancingSceneIndex::new(stage_index);
    let copy = p("/world/coral/Prototypes/staghorn/geo/ForInstancer9b6a0f1ced50293a");
    assert_eq!(
        read(index.as_ref(), &p("/world/coral"), "instancerTopology/prototypes"),
        Some(Value::PathVec(vec![copy.clone()])),
        "the instancer still names the copy"
    );
    let prim = index.prim(&copy);
    assert!(prim.is_defined());
    assert_eq!(
        prim.ty, "",
        "an instance proxy copies as an empty prim, as in UsdImaging 2.0"
    );
    assert!(prim.data.unwrap().names().is_empty());
    assert!(index.children(&copy).is_empty());
    assert!(index.children(&copy.parent().unwrap()).contains(&copy));
}

#[test]
fn a_copy_under_an_invisible_scope_is_visible() {
    let (_dir, usd) = stage(PROXIES_AND_SCOPES);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let flat = moho::filter::FlatteningSceneIndex::new(PointInstancingSceneIndex::new(stage_index));
    let visible = |path: &str| read(flat.as_ref(), &p(path), "visibility/visibility");
    let copy = "/world/rocks/Prototypes/rock/ForInstancera911279021ff5cc5";
    assert_eq!(visible(copy), Some(Value::Bool(true)), "the copy root is visible");
    assert_eq!(
        visible("/world/rocks/Prototypes/rock"),
        Some(Value::Bool(false)),
        "the original stays invisible"
    );
    assert_eq!(
        visible("/world/rocks/Prototypes/hidden/ForInstancera911279021ff5cc5"),
        Some(Value::Bool(false)),
        "a prototype's own visibility is kept"
    );
    assert_eq!(
        read(flat.as_ref(), &p(copy), "xform/resetXformStack"),
        Some(Value::Bool(true))
    );
}

#[test]
fn a_prototype_records_its_copies() {
    let (_dir, usd) = stage(PEBBLES);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let index = PointInstancingSceneIndex::new(stage_index.clone());
    let hash = "ForInstancerad10eba5b2d19a4d";
    let round = p("/world/scatter/protos/round");
    assert_eq!(
        read(
            index.as_ref(),
            &round,
            &format!("__usdPrimInfo/piPropagatedPrototypes/{hash}")
        ),
        Some(Value::PathVec(vec![round.append_path(hash).unwrap()]))
    );
    assert_eq!(
        read(index.as_ref(), &round, "__usdPrimInfo/specifier"),
        Some(Value::Token(Token::from("def"))),
        "the stage's prim info is still there"
    );
    assert!(
        read(
            index.as_ref(),
            &p("/world/scatter/protos/unused"),
            "__usdPrimInfo/piPropagatedPrototypes"
        )
        .is_none()
    );

    // A prototype the instancer stops targeting loses its copy and its
    // record.
    use moho::scene_index::testing::{Notice, RecordingObserver};
    let recorder = RecordingObserver::on(index.as_ref());
    stage_index
        .stage()
        .unwrap()
        .relationship("/world/scatter.prototypes")
        .unwrap()
        .set_targets([p("/world/scatter/protos/flat")])
        .unwrap();
    stage_index.apply_pending_updates();
    let notices = recorder.take();
    let copy = round.append_path(hash).unwrap();
    assert!(
        notices
            .iter()
            .any(|n| matches!(n, Notice::Removed(r) if copy.has_prefix(&r.path))),
        "{notices:?}"
    );
    assert!(!index.prim(&copy).is_defined());
    assert!(
        read(
            index.as_ref(),
            &round,
            &format!("__usdPrimInfo/piPropagatedPrototypes/{hash}")
        )
        .is_none()
    );
    assert_eq!(
        read(
            index.as_ref(),
            &p("/world/scatter"),
            "instancerTopology/prototypes"
        ),
        Some(Value::PathVec(vec![p(&format!(
            "/world/scatter/protos/flat/{hash}"
        ))]))
    );
}
