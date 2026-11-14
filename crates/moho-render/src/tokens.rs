//! The names of the output scene index's prim types and data.

/// The root's data.
pub mod renderer {
    pub const RENDERER: &str = "renderer";
    /// The run state's kind ([`RunState::token`](crate::RunState::token)).
    pub const STATE: &str = "state";
    pub const SAMPLES: &str = "samples";
    /// Absent while the renderer does not know it.
    pub const PROGRESS: &str = "progress";
    /// Why the render failed, in the `failed` state.
    pub const FAILURE: &str = "failure";
    /// An open container: each renderer chooses its keys.
    pub const STATS: &str = "stats";
    pub const CAPABILITIES: &str = "capabilities";
}

/// A var's prim, named after `HdRenderBufferSchema`, plus `generation` and
/// `converged`.
pub mod render_buffer {
    /// The prim type, and the container's name.
    pub const RENDER_BUFFER: &str = "renderBuffer";
    pub const FORMAT: &str = "format";
    pub const DIMENSIONS: &str = "dimensions";
    pub const MULTI_SAMPLED: &str = "multiSampled";
    /// The latest snapshot's generation: dirtied for each new snapshot.
    pub const GENERATION: &str = "generation";
    pub const CONVERGED: &str = "converged";
}

/// A product's prim.
pub mod render_product_output {
    /// The prim type, and the container's name.
    pub const RENDER_PRODUCT_OUTPUT: &str = "renderProductOutput";
    /// The scene's product it answers.
    pub const PRODUCT: &str = "product";
    /// The file the renderer wrote, once written.
    pub const WRITTEN: &str = "written";
}

/// The capabilities, as data under `renderer/capabilities`.
pub mod capabilities {
    pub const PAUSE: &str = "pause";
    pub const STOP: &str = "stop";
    pub const PROGRESSIVE: &str = "progressive";
    pub const CONTINUE_ON_EDIT: &str = "continueOnEdit";
    pub const WRITES_PRODUCTS: &str = "writesProducts";
    pub const MATERIAL_CONTEXTS: &str = "materialContexts";
    pub const AOVS: &str = "aovs";
    pub const SETTINGS: &str = "settings";
    pub const COMMANDS: &str = "commands";
    pub const NAME: &str = "name";
    pub const FORMAT: &str = "format";
    pub const CLEAR_VALUE: &str = "clearValue";
    pub const MULTI_SAMPLED: &str = "multiSampled";
    pub const KEY: &str = "key";
    pub const TYPE: &str = "type";
    pub const DEFAULT: &str = "default";
    pub const MIN: &str = "min";
    pub const MAX: &str = "max";
    pub const CHOICES: &str = "choices";
    pub const DESCRIPTION: &str = "description";
    pub const ARGUMENTS: &str = "arguments";
}

/// The synthetic product rendered without active render settings.
pub mod synthetic {
    /// Its path. A var under it is named after the default AOV.
    pub const PRODUCT: &str = "/__moho_render__/product";
}
