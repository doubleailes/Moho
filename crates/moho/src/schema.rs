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
