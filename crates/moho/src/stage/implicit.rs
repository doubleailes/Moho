//! Implicit shapes: each USD shape's attributes, one to one, under a schema
//! named after its Hydra prim type, in the sorted order of UsdImaging's
//! mapped data source. Shapes are not converted to meshes.

use crate::stage::table::{Member, TableAdapter};
use crate::tokens::implicit as t;
use crate::tokens::prim_type as p;

const fn shape(prim_type: &'static str, members: &'static [Member]) -> TableAdapter {
    TableAdapter {
        prim_type,
        schema: prim_type,
        members,
    }
}

pub const SPHERE: TableAdapter = shape(p::SPHERE, &[Member::Attr(t::RADIUS, "radius")]);
pub const CUBE: TableAdapter = shape(p::CUBE, &[Member::Attr(t::SIZE, "size")]);
pub const CONE: TableAdapter = shape(
    p::CONE,
    &[
        Member::Attr(t::AXIS, "axis"),
        Member::Attr(t::HEIGHT, "height"),
        Member::Attr(t::RADIUS, "radius"),
    ],
);
pub const CYLINDER: TableAdapter = shape(
    p::CYLINDER,
    &[
        Member::Attr(t::AXIS, "axis"),
        Member::Attr(t::HEIGHT, "height"),
        Member::Attr(t::RADIUS, "radius"),
    ],
);
pub const CYLINDER_1: TableAdapter = shape(
    p::CYLINDER,
    &[
        Member::Attr(t::AXIS, "axis"),
        Member::Attr(t::HEIGHT, "height"),
        Member::Attr(t::RADIUS_BOTTOM, "radiusBottom"),
        Member::Attr(t::RADIUS_TOP, "radiusTop"),
    ],
);
pub const CAPSULE: TableAdapter = shape(
    p::CAPSULE,
    &[
        Member::Attr(t::AXIS, "axis"),
        Member::Attr(t::HEIGHT, "height"),
        Member::Attr(t::RADIUS, "radius"),
    ],
);
pub const CAPSULE_1: TableAdapter = shape(
    p::CAPSULE,
    &[
        Member::Attr(t::AXIS, "axis"),
        Member::Attr(t::HEIGHT, "height"),
        Member::Attr(t::RADIUS_BOTTOM, "radiusBottom"),
        Member::Attr(t::RADIUS_TOP, "radiusTop"),
    ],
);
pub const PLANE: TableAdapter = shape(
    p::PLANE,
    &[
        Member::Attr(t::AXIS, "axis"),
        Member::Attr(t::DOUBLE_SIDED, "doubleSided"),
        Member::Attr(t::LENGTH, "length"),
        Member::Attr(t::WIDTH, "width"),
    ],
);
