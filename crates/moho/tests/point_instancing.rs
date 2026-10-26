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
    let Some(Value::PathVec(outer)) =
        read(index.as_ref(), &p("/world/outer"), "instancerTopology/prototypes")
    else {
        panic!()
    };
    let tree = &outer[0];
    let leaves = tree.append_path("leaves").unwrap();
    assert_eq!(index.prim(&leaves).ty, "instancer", "the copied inner instancer");
    let Some(Value::PathVec(inner)) = read(index.as_ref(), &leaves, "instancerTopology/prototypes") else {
        panic!()
    };
    assert_eq!(inner.len(), 1);
    let leaf = &inner[0];
    assert!(
        leaf.has_prefix(&leaves) && leaf.as_str().contains("ForInstancer"),
        "the inner copy lives inside the outer copy, under its own instancer: {leaf}"
    );
    assert_eq!(index.prim(leaf).ty, "mesh");
    assert!(index.children(&leaf.parent().unwrap()).contains(leaf));
    assert_eq!(
        read(index.as_ref(), leaf, "instancedBy/paths"),
        Some(Value::PathVec(vec![leaves.clone()]))
    );
    // The inner original inside the outer copy is drawn only through it.
    let original = leaf.parent().unwrap();
    assert_eq!(index.prim(&original).ty, "", "{original}");
    assert_eq!(
        read(index.as_ref(), leaf, "primOrigin/scenePath"),
        Some(Value::PathVec(vec![p(
            "/world/outer/protos/tree/leaves/protos/leaf"
        )]))
    );
}
