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
