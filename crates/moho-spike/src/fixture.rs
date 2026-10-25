//! The generated scene CI runs the spike on (design D5).
//!
//! A root layer holding one `/world` prim with [`ELEMENTS`] children, each
//! payloading its own layer of meshes, in the shape Crust streams: every child
//! of `/world` is a chunk. Each element layer holds a material and
//! [`MESHES_PER_ELEMENT`] meshes with transforms, points, topology and a
//! material binding; the first mesh's `points` is animated.
//!
//! The layers are written as USDA text rather than authored through openusd,
//! so the fixture does not depend on openusd's (unstable) authoring API.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};

/// The children of `/world`, each one payload and one chunk.
pub const ELEMENTS: usize = 8;

/// The meshes in each element's layer.
pub const MESHES_PER_ELEMENT: usize = 3;

/// The prims each element composes: the element itself, `Looks`, its
/// material, and its meshes.
pub const PRIMS_PER_ELEMENT: usize = 3 + MESHES_PER_ELEMENT;

/// Every prim of the fully loaded stage: `/world` and its elements.
pub const PRIM_COUNT: usize = 1 + ELEMENTS * PRIMS_PER_ELEMENT;

/// The root layer's file name in the fixture directory.
pub const ROOT_LAYER: &str = "root.usda";

/// Writes the fixture into `dir` (which must exist) and returns the path of
/// its root layer.
pub fn write(dir: &Path) -> io::Result<PathBuf> {
    for element in 0..ELEMENTS {
        fs::write(dir.join(element_layer(element)), element_usda(element))?;
    }
    let root = dir.join(ROOT_LAYER);
    fs::write(&root, root_usda())?;
    Ok(root)
}

/// The path of element `n`'s prim on the composed stage.
pub fn element_path(n: usize) -> String {
    format!("/world/element_{n}")
}

/// The stage path of the animated mesh inside element `n`.
pub fn animated_mesh_path(n: usize) -> String {
    format!("{}/mesh_0", element_path(n))
}

fn element_layer(n: usize) -> String {
    format!("element_{n}.usda")
}

fn root_usda() -> String {
    let mut s = String::from(
        "#usda 1.0\n(\n    defaultPrim = \"world\"\n    startTimeCode = 1\n    endTimeCode = 3\n)\n\n\
         def Xform \"world\"\n{\n",
    );
    for n in 0..ELEMENTS {
        let _ = write!(
            s,
            "    def Xform \"element_{n}\" (\n        payload = @./{layer}@</element>\n    )\n    {{\n        \
             double3 xformOp:translate = ({x}, 0, 0)\n        \
             uniform token[] xformOpOrder = [\"xformOp:translate\"]\n    }}\n",
            layer = element_layer(n),
            x = n * 10,
        );
    }
    s.push_str("}\n");
    s
}

fn element_usda(element: usize) -> String {
    let mut s = String::from(
        "#usda 1.0\n(\n    defaultPrim = \"element\"\n)\n\ndef Xform \"element\"\n{\n    \
         def Scope \"Looks\"\n    {\n        def Material \"mat\"\n        {\n        }\n    }\n",
    );
    for mesh in 0..MESHES_PER_ELEMENT {
        // Distinct values per element and mesh, so that a strategy reading the
        // wrong chunk cannot produce the right digest.
        let offset = (element * MESHES_PER_ELEMENT + mesh) as f32;
        let quad = |lift: f32| {
            format!(
                "[({o}, 0, {l}), ({p}, 0, {l}), ({p}, 1, {l}), ({o}, 1, {l})]",
                o = offset,
                p = offset + 1.0,
                l = lift
            )
        };
        let points = if mesh == 0 {
            format!(
                "point3f[] points.timeSamples = {{\n            1: {},\n            3: {},\n        }}",
                quad(0.0),
                quad(2.0)
            )
        } else {
            format!("point3f[] points = {}", quad(0.0))
        };
        let _ = write!(
            s,
            "    def Mesh \"mesh_{mesh}\" (\n        prepend apiSchemas = [\"MaterialBindingAPI\"]\n    )\n    {{\n        \
             double3 xformOp:translate = (0, {mesh}, 0)\n        \
             float3 xformOp:rotateXYZ = (0, {rot}, 0)\n        \
             uniform token[] xformOpOrder = [\"xformOp:translate\", \"xformOp:rotateXYZ\"]\n        \
             {points}\n        \
             int[] faceVertexCounts = [4]\n        \
             int[] faceVertexIndices = [0, 1, 2, 3]\n        \
             rel material:binding = </element/Looks/mat>\n    }}\n",
            rot = element * 15,
        );
    }
    s.push_str("}\n");
    s
}

/// A directory under the system temporary directory, removed on drop.
#[derive(Debug)]
pub struct TempDir(PathBuf);

impl TempDir {
    /// Creates a fresh, empty directory.
    pub fn new() -> io::Result<Self> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("moho-spike-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir(&dir)?;
        Ok(Self(dir))
    }

    /// The directory's path.
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
