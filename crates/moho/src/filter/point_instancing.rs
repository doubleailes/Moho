//! `PointInstancingSceneIndex`: point instancers' prototypes re-rooted as
//! UsdImaging's point-instancer prototype propagation does.
//!
//! Each prototype `P` a point instancer `I` targets is copied to
//! `P/ForInstancer<hash of I>`: the copy's root resets the transform stack,
//! and every prim of the copy names `I` in `instancedBy`. The instancer's
//! `instancerTopology/prototypes` names the copies. Every other prim under
//! the instancer, the original prototypes included, becomes typeless, so it
//! is not drawn on its own. Native instancers (which carry
//! `instanceLocations`) are left alone.
//!
//! A prototype target inside a native instance (an instance proxy, which the
//! index does not hold) is copied from the same prim in that instance's
//! propagated prototype, to `I/ForInstancer<hash>/prototype<i>`: under the
//! instancer, so the native instances of that prototype do not draw it.
//!
//! An instancer inside a prototype is re-instanced in every copy, as Hydra's
//! propagation does: the copy of a nested instancer gets copies of its own
//! prototypes (inside the enclosing copy), named by its output path, and a
//! copy's path values are re-rooted level by level.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::{Rc, Weak};

use openusd::sdf::{self, Value};
use openusd::tf::Token;

use crate::data_source::retained::{RetainedContainer, block, container, value};
use crate::data_source::{Container, ContainerHandle, DataSource, Overlay, locate};
use crate::locator::Locator;
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
};
use crate::schema::{XformSchema, member_container};
use crate::tokens::{geom_model, instancing as t, prim_type, usd_prim_info, xform};

struct Instancer {
    /// The instancer's output path: its input path for a top-level one, or
    /// a path inside another instancer's copy for a nested one.
    path: sdf::Path,
    /// `(copy root, original prototype)`, in prototype order, both output
    /// paths.
    copies: Vec<(sdf::Path, sdf::Path)>,
}

/// One step from a path inside a copy to the namespace the copy was made
/// from.
struct Step<'m> {
    instancer: &'m Instancer,
    copy: &'m sdf::Path,
    original: &'m sdf::Path,
}

#[derive(Default)]
struct Model {
    /// Top-level instancers first, then the instancers found inside copies,
    /// in the order they were found: a copy's original always lies outside
    /// the copies made after it.
    instancers: Vec<Instancer>,
    /// Children the model adds beside the copies hosted at their originals:
    /// an instancer's `ForInstancer<hash>` scope, and the copies of its
    /// prototypes that are instance proxies, under that scope.
    extra_children: std::collections::BTreeMap<sdf::Path, Vec<sdf::Path>>,
    /// The prims whose descendants are unrenderable (UsdImaging's
    /// `_instancersAndOvers`): instancers and `over` prims, output paths,
    /// found over the whole stage and in every copy.
    instancers_and_overs: BTreeSet<sdf::Path>,
}

impl Model {
    /// The innermost copy `out` lies in.
    fn innermost(&self, out: &sdf::Path) -> Option<Step<'_>> {
        let mut best: Option<Step<'_>> = None;
        for instancer in &self.instancers {
            for (copy, original) in &instancer.copies {
                if out.has_prefix(copy)
                    && best
                        .as_ref()
                        .is_none_or(|b| copy.element_count() > b.copy.element_count())
                {
                    best = Some(Step {
                        instancer,
                        copy,
                        original,
                    });
                }
            }
        }
        best
    }

    /// The input path `out` reads, with the copies it goes through,
    /// innermost first.
    fn resolve(&self, out: &sdf::Path) -> Option<(sdf::Path, Vec<Step<'_>>)> {
        let mut path = out.clone();
        let mut steps = Vec::new();
        while let Some(step) = self.innermost(&path) {
            path = path.replace_prefix(step.copy, step.original)?;
            steps.push(step);
        }
        Some((path, steps))
    }

    /// Every output path an input path appears at: itself, and its image in
    /// every copy of a prototype holding it, nested copies included.
    fn outputs(&self, input: &sdf::Path) -> Vec<sdf::Path> {
        let mut out = vec![input.clone()];
        for instancer in &self.instancers {
            for (copy, original) in &instancer.copies {
                let found: Vec<sdf::Path> = out
                    .iter()
                    .filter(|p| p.has_prefix(original))
                    .filter_map(|p| p.replace_prefix(original, copy))
                    .collect();
                out.extend(found);
            }
        }
        out
    }

    /// The copies hosted at `out` (the copy roots whose original is `out`),
    /// and the scope and proxy copies the model adds under it.
    fn hosted(&self, out: &sdf::Path) -> impl Iterator<Item = &sdf::Path> {
        self.instancers
            .iter()
            .flat_map(|i| i.copies.iter())
            .filter(move |(_, original)| original == out)
            .map(|(copy, _)| copy)
            .chain(self.extra_children.get(out).into_iter().flatten())
    }

    /// Whether `out` is an instancer's `ForInstancer<hash>` scope for proxy
    /// prototypes.
    fn is_scope(&self, out: &sdf::Path) -> bool {
        self.instancers
            .iter()
            .any(|i| self.extra_children.get(&i.path).is_some_and(|c| c.contains(out)))
    }

    /// Whether `out` lies strictly under an instancer or an `over`, as
    /// UsdImaging's point instancer prototype scene index judges it: over
    /// the whole stage for a prim in no copy, and from the copy's root down
    /// for a prim in a copy (a copy does not see what lies above its
    /// original).
    fn unrenderable(&self, out: &sdf::Path) -> bool {
        let scope = self.innermost(out).map(|step| step.copy.clone());
        out.strict_ancestors()
            .filter(|a| scope.as_ref().is_none_or(|root| a.has_prefix(root)))
            .any(|a| self.instancers_and_overs.contains(&a))
    }
}

/// Whether a prim hides its descendants: an instancer, or a prim whose
/// specifier is `over` (UsdImaging's `_IsOverOrInstance`).
fn is_over_or_instancer(prim: &Prim) -> bool {
    if prim.ty == prim_type::INSTANCER {
        return true;
    }
    let specifier = prim
        .data
        .as_ref()
        .and_then(|d| {
            locate(
                d,
                &Locator::new([usd_prim_info::USD_PRIM_INFO, usd_prim_info::SPECIFIER]),
            )
        })
        .and_then(|d| d.value());
    matches!(specifier, Some(Value::Token(t)) if t == usd_prim_info::OVER)
}

/// A type the whole-stage pass empties (UsdImaging's
/// `IsRenderablePrimType`): any but the empty type and `material`.
fn is_renderable(ty: &Token) -> bool {
    !ty.as_str().is_empty() && ty != prim_type::MATERIAL
}

/// A prim under an instancer or an `over` made unrenderable, as UsdImaging's
/// `_MakeUnrenderable`: an empty type unless it is not renderable anyway,
/// `__usdPrimInfo/niPrototypePath` blocked so native instancing does not
/// aggregate it, and `geomModel/applyDrawMode` false.
fn make_unrenderable(prim: Prim) -> Prim {
    let ty = if is_renderable(&prim.ty) {
        Token::default()
    } else {
        prim.ty
    };
    let Some(data) = prim.data else {
        return Prim { ty, data: None };
    };
    let overlay = RetainedContainer::new([
        (
            usd_prim_info::USD_PRIM_INFO,
            container([(usd_prim_info::NI_PROTOTYPE_PATH, block())]),
        ),
        (
            geom_model::GEOM_MODEL,
            container([(geom_model::APPLY_DRAW_MODE, value(false))]),
        ),
    ])
    .into_handle();
    Prim {
        ty,
        data: Some(Overlay::handle([overlay, data])),
    }
}

/// A scene index re-rooting point instancers' prototypes.
pub struct PointInstancingSceneIndex {
    input: SceneIndexHandle,
    me: Weak<PointInstancingSceneIndex>,
    model: RefCell<Option<Rc<Model>>>,
    observers: Observers,
}

fn paths(value: Option<Value>) -> Vec<sdf::Path> {
    match value {
        Some(Value::PathVec(v)) => v,
        _ => Vec::new(),
    }
}

/// The copies of a point instancer at `path` whose topology is `data`'s, or
/// `None` for a prim that is not a point instancer (a native instancer
/// carries `instanceLocations`).
fn instancer_at(path: &sdf::Path, data: &ContainerHandle) -> Option<Instancer> {
    let topology = member_container(data, t::INSTANCER_TOPOLOGY)?;
    if topology.get(&Token::from(t::INSTANCE_LOCATIONS)).is_some() {
        return None;
    }
    let mut h = DefaultHasher::new();
    path.as_str().hash(&mut h);
    let suffix = format!("ForInstancer{:016x}", h.finish());
    let prototypes = paths(topology.get(&Token::from(t::PROTOTYPES)).and_then(|d| d.value()));
    let copies = prototypes
        .into_iter()
        .filter_map(|p| Some((p.append_path(suffix.as_str()).ok()?, p)))
        .collect();
    Some(Instancer {
        path: path.clone(),
        copies,
    })
}

impl PointInstancingSceneIndex {
    /// A point-instancing index over `input`.
    pub fn new(input: SceneIndexHandle) -> Rc<Self> {
        let index = Rc::new_cyclic(|me| Self {
            input: input.clone(),
            me: me.clone(),
            model: RefCell::new(None),
            observers: Observers::default(),
        });
        let observer: Weak<dyn Observer> = index.me.clone();
        input.add_observer(observer);
        index
    }

    /// `out`'s input data, re-rooted through every copy it lies in,
    /// outermost first, as Hydra's point instancer propagation re-roots each
    /// level.
    fn data_at(&self, model: &Model, out: &sdf::Path) -> Option<(Prim, Vec<(sdf::Path, sdf::Path)>)> {
        let (input_path, steps) = model.resolve(out)?;
        let input = self.input.prim(&input_path);
        let maps: Vec<(sdf::Path, sdf::Path)> = steps
            .iter()
            .rev()
            .map(|s| (s.original.clone(), s.copy.clone()))
            .collect();
        Some((input, maps))
    }

    /// The children `out` has in the input, through the copies it lies in,
    /// without the copies hosted at it.
    fn natural_children(&self, model: &Model, out: &sdf::Path) -> Vec<sdf::Path> {
        match model.innermost(out) {
            Some(step) => {
                let Some(original) = out.replace_prefix(step.copy, step.original) else {
                    return Vec::new();
                };
                self.natural_children(model, &original)
                    .into_iter()
                    .filter_map(|c| c.replace_prefix(&original, out))
                    .collect()
            }
            None => self.input.children(out),
        }
    }

    /// `out`'s data in the model's namespace (re-rooted through its copies).
    fn data_in(&self, model: &Model, out: &sdf::Path) -> Option<ContainerHandle> {
        let (input, maps) = self.data_at(model, out)?;
        let data = input.data?;
        Some(maps.iter().fold(data, |d, (from, to)| {
            crate::filter::rerooting::rerooted(d, from, to)
        }))
    }

    /// For a prototype target inside a native instance (an instance proxy,
    /// which the index does not hold), the same prim in that instance's
    /// propagated prototype: the nearest ancestor carrying `instance` names
    /// its instancer, whose topology names the propagated prototype.
    fn through_instance(&self, model: &Model, target: &sdf::Path) -> Option<sdf::Path> {
        for ancestor in target.strict_ancestors_below_root() {
            let Some(data) = self.data_in(model, &ancestor) else {
                continue;
            };
            let Some(instance) = member_container(&data, t::INSTANCE) else {
                continue;
            };
            let instancer = paths(instance.get(&Token::from(t::INSTANCER)).and_then(|d| d.value()))
                .into_iter()
                .next()?;
            let topology = self
                .data_in(model, &instancer)
                .and_then(|d| member_container(&d, t::INSTANCER_TOPOLOGY))?;
            let prototype = paths(topology.get(&Token::from(t::PROTOTYPES)).and_then(|d| d.value()))
                .into_iter()
                .next()?;
            return target.replace_prefix(&ancestor, &prototype);
        }
        None
    }

    /// Adds `instancer` to `model`, resolving its prototypes that are
    /// instance proxies through native instancing's propagated prototypes:
    /// their copies go under the instancer's `ForInstancer<hash>` scope, not
    /// under the propagated prototype (whose instances would draw them).
    fn add_instancer(&self, model: &mut Model, mut instancer: Instancer) {
        let mut scope_children = Vec::new();
        // `I/ForInstancer<hash>`, or `ForInstancer<hash>_<n>` should the
        // instancer already have a child of that name.
        let taken = self.natural_children(model, &instancer.path);
        let scope = instancer
            .copies
            .first()
            .and_then(|(copy, _)| copy.name())
            .and_then(|suffix| {
                (0..)
                    .map(|n| {
                        if n == 0 {
                            suffix.to_owned()
                        } else {
                            format!("{suffix}_{n}")
                        }
                    })
                    .filter_map(|name| instancer.path.append_path(name.as_str()).ok())
                    .find(|candidate| !taken.contains(candidate))
            });
        for (i, (copy, original)) in instancer.copies.iter_mut().enumerate() {
            if self.data_in(model, original).is_some() {
                continue;
            }
            let (Some(resolved), Some(scope)) = (self.through_instance(model, original), &scope) else {
                continue;
            };
            let Ok(relocated) = scope.append_path(format!("prototype{i}").as_str()) else {
                continue;
            };
            *copy = relocated.clone();
            *original = resolved;
            scope_children.push(relocated);
        }
        if let Some(scope) = scope.filter(|_| !scope_children.is_empty()) {
            model
                .extra_children
                .entry(instancer.path.clone())
                .or_default()
                .push(scope.clone());
            model
                .extra_children
                .entry(scope)
                .or_default()
                .extend(scope_children);
        }
        model.instancers.push(instancer);
    }

    fn model(&self) -> Rc<Model> {
        if let Some(model) = self.model.borrow().as_ref() {
            return model.clone();
        }
        let mut model = Model::default();
        // The whole stage, parents first. An instancer under an instancer or
        // an `over` has an empty type there, so it is propagated only
        // through the copies holding it.
        let mut stack = vec![sdf::Path::abs_root()];
        while let Some(path) = stack.pop() {
            let prim = self.input.prim(&path);
            let under = model.unrenderable(&path);
            if is_over_or_instancer(&prim) {
                model.instancers_and_overs.insert(path.clone());
            }
            if !under
                && let Some(data) = prim.data
                && let Some(instancer) = instancer_at(&path, &data)
            {
                self.add_instancer(&mut model, instancer);
            }
            stack.extend(self.input.children(&path));
        }
        // Instancers inside copies get copies of their own, recursively, as
        // Hydra's propagation re-instances nested point instancers.
        let mut next = 0;
        while next < model.instancers.len() {
            let roots: Vec<sdf::Path> = model.instancers[next]
                .copies
                .iter()
                .map(|(c, _)| c.clone())
                .collect();
            next += 1;
            for root in roots {
                let mut stack = vec![root];
                while let Some(path) = stack.pop() {
                    if model.instancers.iter().any(|i| i.path == path) {
                        model.instancers_and_overs.insert(path.clone());
                        continue;
                    }
                    if let Some((input, maps)) = self.data_at(&model, &path) {
                        if is_over_or_instancer(&input) {
                            model.instancers_and_overs.insert(path.clone());
                        }
                        if let Some(data) = input.data {
                            let data = maps.iter().fold(data, |d, (from, to)| {
                                crate::filter::rerooting::rerooted(d, from, to)
                            });
                            if let Some(instancer) = instancer_at(&path, &data) {
                                self.add_instancer(&mut model, instancer);
                                continue;
                            }
                        }
                    }
                    if model.instancers_and_overs.contains(&path) {
                        // Nothing under an `over` is drawn, instancers
                        // included.
                        continue;
                    }
                    stack.extend(self.natural_children(&model, &path));
                }
            }
        }
        let model = Rc::new(model);
        *self.model.borrow_mut() = Some(model.clone());
        model
    }

    /// Every prim of every copy, and the scopes holding proxy copies,
    /// parents first.
    fn copy_paths(&self, model: &Model) -> Vec<sdf::Path> {
        let mut out = std::collections::BTreeSet::new();
        for instancer in &model.instancers {
            let roots = instancer
                .copies
                .iter()
                .map(|(copy, _)| copy)
                .chain(model.extra_children.get(&instancer.path).into_iter().flatten());
            for root in roots {
                let mut stack = vec![root.clone()];
                while let Some(path) = stack.pop() {
                    if !out.insert(path.clone()) {
                        continue;
                    }
                    stack.extend(self.children(&path));
                }
            }
        }
        let mut out: Vec<sdf::Path> = out.into_iter().collect();
        out.sort_by_key(sdf::Path::element_count);
        out
    }

    fn resync(&self, old: Option<Rc<Model>>) {
        let removed: Vec<Removed> = old
            .iter()
            .flat_map(|m| {
                m.instancers
                    .iter()
                    .flat_map(|i| {
                        // The scopes holding proxy copies go with them.
                        i.copies
                            .iter()
                            .map(|(c, _)| c)
                            .chain(m.extra_children.get(&i.path).into_iter().flatten())
                            .map(|path| Removed { path: path.clone() })
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        let new = self.model();
        let added: Vec<Added> = self
            .copy_paths(&new)
            .into_iter()
            .map(|path| {
                let ty = self.prim(&path).ty;
                Added { path, ty }
            })
            .collect();
        self.observers.send_removed(self, &removed);
        self.observers.send_added(self, &added);
    }

    /// An instancer's data with `instancerTopology/prototypes` naming its
    /// copies.
    fn with_copies(instancer: &Instancer, local: ContainerHandle) -> ContainerHandle {
        let copies: Vec<sdf::Path> = instancer.copies.iter().map(|(c, _)| c.clone()).collect();
        let topology = member_container(&local, t::INSTANCER_TOPOLOGY).map(|original| -> DataSource {
            DataSource::Container(Rc::new(Overridden {
                local: original,
                overrides: vec![(Token::from(t::PROTOTYPES), value(Value::PathVec(copies)))],
            }))
        });
        let overrides = topology
            .map(|t| vec![(Token::from(t::INSTANCER_TOPOLOGY), t)])
            .unwrap_or_default();
        Rc::new(Overridden { local, overrides })
    }
}

impl SceneIndex for PointInstancingSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        let model = self.model();
        if model.is_scope(path) {
            return Prim::new(
                Token::default(),
                crate::data_source::retained::RetainedContainer::default().into_handle(),
            );
        }
        let Some((input, maps)) = self.data_at(&model, path) else {
            return Prim::undefined();
        };
        let Some(local) = input.data else {
            return input;
        };
        // The copies' path values name the copies, level by level.
        let local = maps.iter().fold(local, |d, (from, to)| {
            crate::filter::rerooting::rerooted(d, from, to)
        });
        let local = match model.instancers.iter().find(|i| i.path == *path) {
            Some(instancer) => Self::with_copies(instancer, local),
            None => local,
        };
        if model.unrenderable(path) {
            // Drawn only through an instancer, or not at all under an
            // `over`.
            return make_unrenderable(Prim {
                ty: input.ty,
                data: Some(local),
            });
        }
        let ty = input.ty;
        let Some(step) = model.innermost(path) else {
            return Prim {
                ty,
                data: Some(local),
            };
        };
        let mut overrides: Vec<(Token, DataSource)> = vec![(
            Token::from(t::INSTANCED_BY),
            container([
                (t::PATHS, value(Value::PathVec(vec![step.instancer.path.clone()]))),
                (t::PROTOTYPE_ROOTS, value(Value::PathVec(vec![step.copy.clone()]))),
            ]),
        )];
        if path == step.copy {
            // The copy's root composes under the instancer's instances, not
            // under its parent: its own transform, reset.
            let mut entries = vec![(xform::RESET_XFORM_STACK, value(true))];
            if let Some(matrix) = XformSchema::get(&local).and_then(|x| x.matrix()) {
                entries.push((xform::MATRIX, DataSource::Sampled(matrix)));
            }
            overrides.push((Token::from(xform::XFORM), container(entries)));
        }
        Prim {
            ty,
            data: Some(Rc::new(Overridden { local, overrides })),
        }
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        let model = self.model();
        let mut children = self.natural_children(&model, path);
        children.extend(model.hosted(path).cloned());
        children
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

impl Observer for PointInstancingSceneIndex {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        let old = self.model.borrow_mut().take();
        let model = self.model();
        // A prim under an instancer or an `over` is added unrenderable.
        let entries: Vec<Added> = entries
            .iter()
            .map(|e| Added {
                path: e.path.clone(),
                ty: if is_renderable(&e.ty) && model.unrenderable(&e.path) {
                    Token::default()
                } else {
                    e.ty.clone()
                },
            })
            .collect();
        self.observers.send_added(self, &entries);
        self.resync(old);
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        let old = self.model.borrow_mut().take();
        self.observers.send_removed(self, entries);
        self.resync(old);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        let topology = Locator::new([t::INSTANCER_TOPOLOGY]);
        if entries.iter().any(|e| e.locators.intersects(&topology)) {
            let old = self.model.borrow_mut().take();
            self.resync(old);
        }
        let model = self.model();
        // A prototype's edits reach its copies, nested ones included.
        let out: Vec<Dirtied> = entries
            .iter()
            .flat_map(|e| {
                model.outputs(&e.path).into_iter().map(|path| Dirtied {
                    path,
                    locators: e.locators.clone(),
                })
            })
            .collect();
        self.observers.send_dirtied(self, &out);
    }

    fn renamed(&self, _: &dyn SceneIndex, entries: &[Renamed]) {
        let old = self.model.borrow_mut().take();
        self.observers.send_renamed(self, entries);
        self.resync(old);
    }
}

/// A container with some members replaced.
struct Overridden {
    local: ContainerHandle,
    overrides: Vec<(Token, DataSource)>,
}

impl Container for Overridden {
    fn names(&self) -> Vec<Token> {
        let mut names = self.local.names();
        for (name, _) in &self.overrides {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        self.overrides
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, s)| s.clone())
            .or_else(|| self.local.get(name))
    }
}
