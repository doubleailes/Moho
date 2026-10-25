//! Typed views over prim containers, one per Hydra schema (`mesh`,
//! `primvars`, `xform`, ...).
//!
//! A view wraps the container a schema's token names and reads its members
//! by name, as Hydra's schema classes do. Every accessor answers `None` when
//! the member is absent.

use openusd::gf;
use openusd::sdf::Value;
use openusd::tf::Token;

use crate::data_source::{ContainerHandle, DataSource, SampledHandle};
use crate::tokens;

/// The data source named `name` in `container`.
pub fn member(container: &ContainerHandle, name: &str) -> Option<DataSource> {
    container.get(&Token::from(name))
}

/// The value of the sampled member `name` at offset 0.
pub fn member_value(container: &ContainerHandle, name: &str) -> Option<Value> {
    member(container, name)?.value()
}

/// The sampled member `name`.
pub fn member_sampled(container: &ContainerHandle, name: &str) -> Option<SampledHandle> {
    member(container, name)?.as_sampled().cloned()
}

/// The container member `name`.
pub fn member_container(container: &ContainerHandle, name: &str) -> Option<ContainerHandle> {
    member(container, name)?.as_container().cloned()
}

/// `HdXformSchema`: a prim's local transform.
pub struct XformSchema(ContainerHandle);

impl XformSchema {
    /// The `xform` schema of a prim's container.
    pub fn get(prim: &ContainerHandle) -> Option<Self> {
        member_container(prim, tokens::xform::XFORM).map(Self)
    }

    /// The local matrix, sampled.
    pub fn matrix(&self) -> Option<SampledHandle> {
        member_sampled(&self.0, tokens::xform::MATRIX)
    }

    /// The local matrix at the index's time.
    pub fn matrix_value(&self) -> Option<gf::Mat4d> {
        match self.matrix()?.value(0.0)? {
            Value::Matrix4d(m) => Some(m),
            _ => None,
        }
    }

    /// Whether the prim ignores its parent's transform.
    pub fn reset_xform_stack(&self) -> Option<bool> {
        match member_value(&self.0, tokens::xform::RESET_XFORM_STACK)? {
            Value::Bool(b) => Some(b),
            _ => None,
        }
    }
}

/// The token a value holds.
pub fn as_token(value: Value) -> Option<Token> {
    match value {
        Value::Token(t) => Some(t),
        _ => None,
    }
}

/// The ints a value holds.
pub fn as_ints(value: Value) -> Option<Vec<i32>> {
    match value {
        Value::IntVec(v) => Some(v),
        _ => None,
    }
}

/// The floats a value holds.
pub fn as_floats(value: Value) -> Option<Vec<f32>> {
    match value {
        Value::FloatVec(v) => Some(v),
        _ => None,
    }
}

/// The bool a value holds.
pub fn as_bool(value: Value) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(b),
        _ => None,
    }
}

/// `HdMeshSchema`.
pub struct MeshSchema(ContainerHandle);

impl MeshSchema {
    pub fn get(prim: &ContainerHandle) -> Option<Self> {
        member_container(prim, tokens::mesh::MESH).map(Self)
    }

    pub fn topology(&self) -> Option<MeshTopologySchema> {
        member_container(&self.0, tokens::mesh::TOPOLOGY).map(MeshTopologySchema)
    }

    pub fn subdivision_scheme(&self) -> Option<Token> {
        member_value(&self.0, tokens::mesh::SUBDIVISION_SCHEME).and_then(as_token)
    }

    pub fn subdivision_tags(&self) -> Option<SubdivisionTagsSchema> {
        member_container(&self.0, tokens::mesh::SUBDIVISION_TAGS).map(SubdivisionTagsSchema)
    }

    pub fn double_sided(&self) -> Option<bool> {
        member_value(&self.0, tokens::mesh::DOUBLE_SIDED).and_then(as_bool)
    }
}

/// `HdMeshTopologySchema`.
pub struct MeshTopologySchema(ContainerHandle);

impl MeshTopologySchema {
    pub fn face_vertex_counts(&self) -> Option<Vec<i32>> {
        member_value(&self.0, tokens::mesh::FACE_VERTEX_COUNTS).and_then(as_ints)
    }

    pub fn face_vertex_indices(&self) -> Option<Vec<i32>> {
        member_value(&self.0, tokens::mesh::FACE_VERTEX_INDICES).and_then(as_ints)
    }

    pub fn hole_indices(&self) -> Option<Vec<i32>> {
        member_value(&self.0, tokens::mesh::HOLE_INDICES).and_then(as_ints)
    }

    pub fn orientation(&self) -> Option<Token> {
        member_value(&self.0, tokens::mesh::ORIENTATION).and_then(as_token)
    }
}

/// `HdSubdivisionTagsSchema`.
pub struct SubdivisionTagsSchema(ContainerHandle);

impl SubdivisionTagsSchema {
    pub fn interpolate_boundary(&self) -> Option<Token> {
        member_value(&self.0, tokens::mesh::INTERPOLATE_BOUNDARY).and_then(as_token)
    }

    pub fn face_varying_linear_interpolation(&self) -> Option<Token> {
        member_value(&self.0, tokens::mesh::FACE_VARYING_LINEAR_INTERPOLATION).and_then(as_token)
    }

    pub fn triangle_subdivision_rule(&self) -> Option<Token> {
        member_value(&self.0, tokens::mesh::TRIANGLE_SUBDIVISION_RULE).and_then(as_token)
    }

    pub fn corner_indices(&self) -> Option<Vec<i32>> {
        member_value(&self.0, tokens::mesh::CORNER_INDICES).and_then(as_ints)
    }

    pub fn corner_sharpnesses(&self) -> Option<Vec<f32>> {
        member_value(&self.0, tokens::mesh::CORNER_SHARPNESSES).and_then(as_floats)
    }

    pub fn crease_indices(&self) -> Option<Vec<i32>> {
        member_value(&self.0, tokens::mesh::CREASE_INDICES).and_then(as_ints)
    }

    pub fn crease_lengths(&self) -> Option<Vec<i32>> {
        member_value(&self.0, tokens::mesh::CREASE_LENGTHS).and_then(as_ints)
    }

    pub fn crease_sharpnesses(&self) -> Option<Vec<f32>> {
        member_value(&self.0, tokens::mesh::CREASE_SHARPNESSES).and_then(as_floats)
    }
}

/// `HdPrimvarsSchema`: a prim's primvars by name.
pub struct PrimvarsSchema(ContainerHandle);

impl PrimvarsSchema {
    pub fn get(prim: &ContainerHandle) -> Option<Self> {
        member_container(prim, tokens::primvars::PRIMVARS).map(Self)
    }

    /// The names of the prim's primvars.
    pub fn names(&self) -> Vec<Token> {
        self.0.names()
    }

    /// The primvar `name`.
    pub fn primvar(&self, name: &str) -> Option<PrimvarSchema> {
        member_container(&self.0, name).map(PrimvarSchema)
    }
}

/// `HdPrimvarSchema`: one primvar.
pub struct PrimvarSchema(ContainerHandle);

impl PrimvarSchema {
    /// The value of an unindexed primvar.
    pub fn primvar_value(&self) -> Option<SampledHandle> {
        member_sampled(&self.0, tokens::primvars::PRIMVAR_VALUE)
    }

    /// The values of an indexed primvar, before its indices apply.
    pub fn indexed_primvar_value(&self) -> Option<SampledHandle> {
        member_sampled(&self.0, tokens::primvars::INDEXED_PRIMVAR_VALUE)
    }

    pub fn indices(&self) -> Option<Vec<i32>> {
        member_value(&self.0, tokens::primvars::INDICES).and_then(as_ints)
    }

    pub fn interpolation(&self) -> Option<Token> {
        member_value(&self.0, tokens::primvars::INTERPOLATION).and_then(as_token)
    }

    pub fn role(&self) -> Option<Token> {
        member_value(&self.0, tokens::primvars::ROLE).and_then(as_token)
    }

    pub fn element_size(&self) -> Option<i32> {
        match member_value(&self.0, tokens::primvars::ELEMENT_SIZE)? {
            Value::Int(i) => Some(i),
            _ => None,
        }
    }

    pub fn color_space(&self) -> Option<Value> {
        member_value(&self.0, tokens::primvars::COLOR_SPACE)
    }

    /// The value with its indices applied (Hydra's
    /// `GetFlattenedPrimvarValue`), at the index's time plus `offset`: the
    /// plain value of an unindexed primvar, else, for each index, the
    /// `elementSize` values it selects. `None` when an index is out of
    /// range.
    pub fn flattened_value(&self, offset: f32) -> Option<Value> {
        if let Some(value) = self.primvar_value() {
            return value.value(offset);
        }
        let values = self.indexed_primvar_value()?.value(offset)?;
        let indices = member_sampled(&self.0, tokens::primvars::INDICES)?
            .value(offset)
            .and_then(as_ints)?;
        let size = usize::try_from(self.element_size().unwrap_or(1)).ok()?.max(1);
        gather(values, &indices, size)
    }
}

/// `values` gathered through `indices`, `size` elements per index.
fn gather(values: Value, indices: &[i32], size: usize) -> Option<Value> {
    fn pick<T: Clone>(v: &[T], indices: &[i32], size: usize) -> Option<Vec<T>> {
        let mut out = Vec::with_capacity(indices.len() * size);
        for &i in indices {
            let start = usize::try_from(i).ok()? * size;
            out.extend_from_slice(v.get(start..start + size)?);
        }
        Some(out)
    }
    macro_rules! gather {
        ($($variant:ident),*) => {
            match values {
                $(Value::$variant(v) => pick(&v, indices, size).map(Value::$variant),)*
                other => Some(other),
            }
        };
    }
    gather!(
        BoolVec,
        UcharVec,
        IntVec,
        UintVec,
        Int64Vec,
        Uint64Vec,
        HalfVec,
        FloatVec,
        DoubleVec,
        StringVec,
        TokenVec,
        AssetPathVec,
        QuathVec,
        QuatfVec,
        QuatdVec,
        Vec2hVec,
        Vec2fVec,
        Vec2dVec,
        Vec2iVec,
        Vec3hVec,
        Vec3fVec,
        Vec3dVec,
        Vec3iVec,
        Vec4hVec,
        Vec4fVec,
        Vec4dVec,
        Vec4iVec,
        Matrix2dVec,
        Matrix3dVec,
        Matrix4dVec
    )
}

/// `HdVisibilitySchema`.
pub struct VisibilitySchema(ContainerHandle);

impl VisibilitySchema {
    pub fn get(prim: &ContainerHandle) -> Option<Self> {
        member_container(prim, tokens::imageable::VISIBILITY).map(Self)
    }

    /// `false` when the prim authors `invisible`, `None` otherwise.
    pub fn visibility(&self) -> Option<bool> {
        member_value(&self.0, tokens::imageable::VISIBILITY).and_then(as_bool)
    }
}

/// `HdPurposeSchema`.
pub struct PurposeSchema(ContainerHandle);

impl PurposeSchema {
    pub fn get(prim: &ContainerHandle) -> Option<Self> {
        member_container(prim, tokens::imageable::PURPOSE).map(Self)
    }

    pub fn purpose(&self) -> Option<Token> {
        member_value(&self.0, tokens::imageable::PURPOSE).and_then(as_token)
    }

    pub fn inheritable(&self) -> Option<bool> {
        member_value(&self.0, tokens::imageable::INHERITABLE).and_then(as_bool)
    }

    pub fn fallback(&self) -> Option<Token> {
        member_value(&self.0, tokens::imageable::FALLBACK).and_then(as_token)
    }
}

/// `HdExtentSchema`.
pub struct ExtentSchema(ContainerHandle);

impl ExtentSchema {
    pub fn get(prim: &ContainerHandle) -> Option<Self> {
        member_container(prim, tokens::imageable::EXTENT).map(Self)
    }

    pub fn min(&self) -> Option<gf::Vec3d> {
        match member_value(&self.0, tokens::imageable::MIN)? {
            Value::Vec3d(v) => Some(v),
            _ => None,
        }
    }

    pub fn max(&self) -> Option<gf::Vec3d> {
        match member_value(&self.0, tokens::imageable::MAX)? {
            Value::Vec3d(v) => Some(v),
            _ => None,
        }
    }
}
