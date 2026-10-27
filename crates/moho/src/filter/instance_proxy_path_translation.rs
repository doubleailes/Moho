//! `InstanceProxyPathTranslationSceneIndex`: path values that name a prim
//! inside a native instance (an instance proxy, which the scene index does
//! not hold) translated to the same prim in the instance's propagated
//! prototype, as `UsdImaging_InstanceProxyPathTranslationSceneIndex` does.
//!
//! Only the data sources it is configured with are translated
//! (`UsdImagingCreateSceneIndices` gives it `usdMaterialBindings`), and in
//! them every path value, at any depth. A path is translated as
//! `_NewTranslatePath` (UsdImaging's default,
//! `USDIMAGING_CORRECT_PROXY_PATH_TRANSLATION`): while it names no prim of
//! the input, its nearest ancestor that is a prim and carries `instance`
//! data has its prefix replaced by the prototype root its instancer's
//! `instancerTopology/prototypes` names at `prototypeIndex`, and the walk
//! starts again; a prototype lying under the instance replaces nothing. The
//! walk gives up after a thousand steps.

use std::rc::{Rc, Weak};

use openusd::sdf::{self, Value};
use openusd::tf::Token;

use crate::data_source::{
    Container, ContainerHandle, DataSource, Sampled, SampledHandle, Vector, VectorHandle,
};
use crate::locator::Locator;
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
};
use crate::tokens::instancing as t;

const MAX_ITERATIONS: usize = 1000;

/// A scene index translating instance proxy paths in some of its prims'
/// data.
pub struct InstanceProxyPathTranslationSceneIndex {
    input: SceneIndexHandle,
    names: Rc<Vec<Token>>,
    me: Weak<InstanceProxyPathTranslationSceneIndex>,
    observers: Observers,
}

impl InstanceProxyPathTranslationSceneIndex {
    /// An index over `input` translating the path values under the prim
    /// data sources `names`.
    pub fn new<'a>(input: SceneIndexHandle, names: impl IntoIterator<Item = &'a str>) -> Rc<Self> {
        let index = Rc::new_cyclic(|me| Self {
            input: input.clone(),
            names: Rc::new(names.into_iter().map(Token::from).collect()),
            me: me.clone(),
            observers: Observers::default(),
        });
        let observer: Weak<dyn Observer> = index.me.clone();
        input.add_observer(observer);
        index
    }
}

/// The prototype root an instance prim's `instance` data names
/// (`_GetPrototypePath`).
fn prototype_path(index: &dyn SceneIndex, instance: &ContainerHandle) -> Option<sdf::Path> {
    let instance = match instance.get(&Token::from(t::INSTANCE))? {
        DataSource::Container(c) => c,
        _ => return None,
    };
    let instancer = match instance.get(&Token::from(t::INSTANCER))?.value()? {
        Value::PathVec(v) => v.into_iter().next()?,
        _ => return None,
    };
    let prototype_index = match instance.get(&Token::from(t::PROTOTYPE_INDEX))?.value()? {
        Value::Int(i) => usize::try_from(i).ok()?,
        _ => return None,
    };
    let topology = index.prim(&instancer).data?;
    match crate::data_source::locate(&topology, &Locator::new([t::INSTANCER_TOPOLOGY, t::PROTOTYPES]))?
        .value()?
    {
        Value::PathVec(prototypes) => prototypes.into_iter().nth(prototype_index),
        _ => None,
    }
}

/// `_NewTranslatePath`.
fn translate(path: &sdf::Path, index: &dyn SceneIndex) -> sdf::Path {
    let mut result = path.clone();
    for _ in 0..MAX_ITERATIONS {
        if index.prim(&result).is_defined() {
            break;
        }
        let mut again = false;
        for ancestor in result.ancestors_below_root() {
            let Some(data) = index.prim(&ancestor).data else {
                continue;
            };
            let Some(prototype) = prototype_path(index, &data) else {
                continue;
            };
            // A prototype under its own instance (a point instancer using a
            // native instance's prototype) replaces nothing.
            if prototype.has_prefix(&ancestor) {
                continue;
            }
            if let Some(translated) = result.replace_prefix(&ancestor, &prototype) {
                result = translated;
                again = true;
                break;
            }
        }
        if !again {
            break;
        }
    }
    result
}

type Input = Weak<dyn SceneIndex>;

fn translate_source(source: DataSource, input: &Input) -> DataSource {
    match source {
        DataSource::Container(inner) => DataSource::Container(Rc::new(Translated {
            inner,
            input: input.clone(),
        })),
        DataSource::Vector(inner) => DataSource::Vector(Rc::new(TranslatedVector {
            inner,
            input: input.clone(),
        })),
        DataSource::Sampled(inner) => DataSource::Sampled(Rc::new(TranslatedSampled {
            inner,
            input: input.clone(),
        })),
        DataSource::Block => DataSource::Block,
    }
}

struct Translated {
    inner: ContainerHandle,
    input: Input,
}

impl Container for Translated {
    fn names(&self) -> Vec<Token> {
        self.inner.names()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        Some(translate_source(self.inner.get(name)?, &self.input))
    }
}

struct TranslatedVector {
    inner: VectorHandle,
    input: Input,
}

impl Vector for TranslatedVector {
    fn len(&self) -> usize {
        self.inner.len()
    }

    fn get(&self, index: usize) -> Option<DataSource> {
        Some(translate_source(self.inner.get(index)?, &self.input))
    }
}

struct TranslatedSampled {
    inner: SampledHandle,
    input: Input,
}

impl Sampled for TranslatedSampled {
    fn value(&self, offset: f32) -> Option<Value> {
        match self.inner.value(offset)? {
            Value::PathVec(paths) => {
                let Some(input) = self.input.upgrade() else {
                    return Some(Value::PathVec(paths));
                };
                Some(Value::PathVec(
                    paths.iter().map(|p| translate(p, input.as_ref())).collect(),
                ))
            }
            other => Some(other),
        }
    }

    fn sample_times(&self, start: f32, end: f32) -> Option<Vec<f32>> {
        self.inner.sample_times(start, end)
    }
}

/// A prim's data, translated under the configured names (`_PrimDs`).
struct PrimData {
    inner: ContainerHandle,
    input: Input,
    names: Rc<Vec<Token>>,
}

impl Container for PrimData {
    fn names(&self) -> Vec<Token> {
        self.inner.names()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let source = self.inner.get(name)?;
        if self.names.contains(name) {
            Some(translate_source(source, &self.input))
        } else {
            Some(source)
        }
    }
}

impl SceneIndex for InstanceProxyPathTranslationSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        let prim = self.input.prim(path);
        let Some(inner) = prim.data else {
            return prim;
        };
        Prim {
            ty: prim.ty,
            data: Some(Rc::new(PrimData {
                inner,
                input: Rc::downgrade(&self.input),
                names: self.names.clone(),
            })),
        }
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        self.input.children(path)
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

impl Observer for InstanceProxyPathTranslationSceneIndex {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        self.observers.send_added(self, entries);
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        self.observers.send_removed(self, entries);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        self.observers.send_dirtied(self, entries);
    }

    fn renamed(&self, _: &dyn SceneIndex, entries: &[Renamed]) {
        self.observers.send_renamed(self, entries);
    }
}
