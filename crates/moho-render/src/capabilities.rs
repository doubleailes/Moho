//! What a renderer can do, declared without rendering, as plain data and as
//! the output index's `renderer/capabilities` (so a UI can build a settings
//! panel by observing the output index alone).

use moho::data_source::DataSource;
use moho::data_source::retained::{RetainedContainer, RetainedSampled, RetainedVector};
use moho::openusd::tf::Token;
use moho::openusd::vt::Value;

use crate::tokens::capabilities as t;
use crate::types::PixelFormat;

/// An AOV the renderer can produce (`HdAovDescriptor`, with its name).
#[derive(Debug, Clone, PartialEq)]
pub struct AovDescriptor {
    /// The AOV's name, as a render var's `sourceName` names it (`color`,
    /// `depth`, `primId`).
    pub name: Token,
    pub format: PixelFormat,
    /// The value a cleared pixel holds.
    pub clear_value: Value,
    pub multi_sampled: bool,
}

/// A setting's type.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingType {
    Bool,
    Int,
    Float,
    String,
    Token,
    /// A token among these.
    Enum(Vec<Token>),
    /// An absolute prim path (a camera, a light filter).
    Path,
    /// A list of absolute prim paths.
    PathList,
}

impl SettingType {
    /// The type's name, as data.
    pub fn token(&self) -> Token {
        Token::new(match self {
            SettingType::Bool => "bool",
            SettingType::Int => "int",
            SettingType::Float => "float",
            SettingType::String => "string",
            SettingType::Token => "token",
            SettingType::Enum(_) => "enum",
            SettingType::Path => "path",
            SettingType::PathList => "pathList",
        })
    }
}

/// A renderer setting (`HdRenderSettingDescriptor`), read from the active
/// render settings' `namespacedSettings` under its full name. Made with
/// [`SettingDescriptor::new`], so a field added later breaks no renderer.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct SettingDescriptor {
    /// The namespaced name (`ri:hider:maxsamples`).
    pub key: Token,
    pub ty: SettingType,
    pub default: Value,
    /// The smallest and largest values, where bounded.
    pub range: Option<(Value, Value)>,
    /// The name a person reads (`Max Samples`): the key unless given.
    pub label: String,
    /// One line on what the setting does; empty unless given.
    pub help: String,
    /// The section a settings panel lists it in; empty unless given.
    pub group: String,
}

impl SettingDescriptor {
    /// The setting `key` of type `ty`, defaulting to `default`, unbounded.
    pub fn new(key: impl Into<Token>, ty: SettingType, default: impl Into<Value>) -> Self {
        let key = key.into();
        Self {
            label: key.as_str().to_owned(),
            key,
            ty,
            default: default.into(),
            range: None,
            help: String::new(),
            group: String::new(),
        }
    }

    /// This setting, named `label` for a person.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// This setting, described by `help`.
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = help.into();
        self
    }

    /// This setting, listed in `group`.
    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group = group.into();
        self
    }

    /// This setting, bounded by `min` and `max`.
    pub fn with_range(mut self, min: impl Into<Value>, max: impl Into<Value>) -> Self {
        self.range = Some((min.into(), max.into()));
        self
    }
}

/// A command the renderer runs (`HdCommandDescriptor`).
#[derive(Debug, Clone, PartialEq)]
pub struct CommandDescriptor {
    pub name: Token,
    pub description: String,
    pub arguments: Vec<SettingDescriptor>,
}

/// What a renderer can do.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Capabilities {
    pub pause: bool,
    pub stop: bool,
    /// Whether frames improve over samples, so convergence and progress mean
    /// something.
    pub progressive: bool,
    /// Whether the renderer can keep its samples after an edit
    /// ([`Invalidation::Continue`](crate::Invalidation::Continue)).
    pub continue_on_edit: bool,
    /// Whether the renderer writes products itself
    /// ([`ProductWriter::Renderer`](crate::ProductWriter::Renderer)).
    pub writes_products: bool,
    /// The material render contexts it reads (`ri`, `mtlx`, `""`).
    pub material_contexts: Vec<Token>,
    /// Its AOVs; the first is the default, rendered without render settings.
    pub aovs: Vec<AovDescriptor>,
    pub settings: Vec<SettingDescriptor>,
    pub commands: Vec<CommandDescriptor>,
}

impl Capabilities {
    /// The default AOV: the first declared.
    pub fn default_aov(&self) -> Option<&AovDescriptor> {
        self.aovs.first()
    }

    /// The command named `name`, if declared.
    pub fn command(&self, name: &Token) -> Option<&CommandDescriptor> {
        self.commands.iter().find(|c| c.name == *name)
    }

    /// The capabilities as data: the flags as booleans, `materialContexts`
    /// as tokens, and `aovs`, `settings` and `commands` as vectors of
    /// containers.
    pub fn to_container(&self) -> RetainedContainer {
        let aovs = self.aovs.iter().map(|aov| {
            RetainedContainer::new([
                (t::NAME, value(aov.name.clone())),
                (t::FORMAT, value(aov.format.token())),
                (t::CLEAR_VALUE, value(aov.clear_value.clone())),
                (t::MULTI_SAMPLED, value(aov.multi_sampled)),
            ])
            .into_data_source()
        });
        let commands = self.commands.iter().map(|command| {
            RetainedContainer::new([
                (t::NAME, value(command.name.clone())),
                (t::DESCRIPTION, value(command.description.clone())),
                (t::ARGUMENTS, settings(&command.arguments)),
            ])
            .into_data_source()
        });
        RetainedContainer::new([
            (t::PAUSE, value(self.pause)),
            (t::STOP, value(self.stop)),
            (t::PROGRESSIVE, value(self.progressive)),
            (t::CONTINUE_ON_EDIT, value(self.continue_on_edit)),
            (t::WRITES_PRODUCTS, value(self.writes_products)),
            (
                t::MATERIAL_CONTEXTS,
                value(Value::TokenVec(self.material_contexts.clone())),
            ),
            (t::AOVS, RetainedVector::new(aovs).into_data_source()),
            (t::SETTINGS, settings(&self.settings)),
            (t::COMMANDS, RetainedVector::new(commands).into_data_source()),
        ])
    }
}

/// A uniform value as a data source.
pub(crate) fn value(v: impl Into<Value>) -> DataSource {
    RetainedSampled::uniform(v).into_data_source()
}

fn settings(descriptors: &[SettingDescriptor]) -> DataSource {
    RetainedVector::new(descriptors.iter().map(|setting| {
        let mut container = RetainedContainer::new([
            (t::KEY, value(setting.key.clone())),
            (t::TYPE, value(setting.ty.token())),
            (t::DEFAULT, value(setting.default.clone())),
            (t::LABEL, value(setting.label.clone())),
            (t::HELP, value(setting.help.clone())),
            (t::GROUP, value(setting.group.clone())),
        ]);
        if let Some((min, max)) = &setting.range {
            container.set(t::MIN, value(min.clone()));
            container.set(t::MAX, value(max.clone()));
        }
        if let SettingType::Enum(choices) = &setting.ty {
            container.set(t::CHOICES, value(Value::TokenVec(choices.clone())));
        }
        container.into_data_source()
    }))
    .into_data_source()
}

#[cfg(test)]
mod tests {
    use moho::data_source::{ContainerHandle, locate};
    use moho::locator::Locator;

    use super::*;

    fn read(container: &ContainerHandle, locator: &str) -> Option<Value> {
        locate(container, &Locator::parse(locator))?.value()
    }

    #[test]
    fn a_setting_reads_back_from_the_container() {
        let capabilities = Capabilities {
            settings: vec![
                SettingDescriptor::new(
                    Token::new("ri:hider:maxsamples"),
                    SettingType::Int,
                    Value::Int(64),
                )
                .with_range(Value::Int(1), Value::Int(4096)),
            ],
            ..Capabilities::default()
        };
        let container = capabilities.to_container().into_handle();
        let settings = locate(&container, &Locator::parse("settings")).unwrap();
        let settings = settings.as_vector().unwrap();
        assert_eq!(settings.len(), 1);
        let setting = settings.get(0).unwrap();
        let setting = setting.as_container().unwrap();
        assert_eq!(
            read(setting, "key"),
            Some(Value::Token(Token::new("ri:hider:maxsamples")))
        );
        assert_eq!(read(setting, "type"), Some(Value::Token(Token::new("int"))));
        assert_eq!(read(setting, "default"), Some(Value::Int(64)));
        assert_eq!(read(setting, "max"), Some(Value::Int(4096)));
    }

    #[test]
    fn a_setting_described_for_a_person() {
        let capabilities = Capabilities {
            settings: vec![
                SettingDescriptor::new(
                    Token::new("ri:hider:maxsamples"),
                    SettingType::Int,
                    Value::Int(64),
                )
                .with_label("Max Samples")
                .with_help("Samples per pixel at most.")
                .with_group("Sampling"),
            ],
            ..Capabilities::default()
        };
        let container = capabilities.to_container().into_handle();
        let settings = locate(&container, &Locator::parse("settings")).unwrap();
        let setting = settings.as_vector().unwrap().get(0).unwrap();
        let setting = setting.as_container().unwrap();
        assert_eq!(read(setting, "label"), Some(Value::String("Max Samples".into())));
        assert_eq!(
            read(setting, "help"),
            Some(Value::String("Samples per pixel at most.".into()))
        );
        assert_eq!(read(setting, "group"), Some(Value::String("Sampling".into())));
    }

    #[test]
    fn a_path_setting_names_its_type() {
        let capabilities = Capabilities {
            settings: vec![
                SettingDescriptor::new(Token::new("ri:dicingCamera"), SettingType::Path, Value::None),
                SettingDescriptor::new(Token::new("ri:lightFilters"), SettingType::PathList, Value::None),
            ],
            ..Capabilities::default()
        };
        let container = capabilities.to_container().into_handle();
        let settings = locate(&container, &Locator::parse("settings")).unwrap();
        let settings = settings.as_vector().unwrap();
        let ty = |i: usize| {
            let setting = settings.get(i).unwrap();
            read(setting.as_container().unwrap(), "type")
        };
        assert_eq!(ty(0), Some(Value::Token(Token::new("path"))));
        assert_eq!(ty(1), Some(Value::Token(Token::new("pathList"))));
    }

    #[test]
    fn an_undescribed_setting() {
        let setting = SettingDescriptor::new(
            Token::new("ri:hider:maxsamples"),
            SettingType::Int,
            Value::Int(64),
        );
        assert_eq!(setting.label, "ri:hider:maxsamples");
        assert!(setting.help.is_empty() && setting.group.is_empty());
    }

    #[test]
    fn flags_and_aovs_read_back() {
        let capabilities = Capabilities {
            pause: true,
            writes_products: true,
            material_contexts: vec![Token::new("ri")],
            aovs: vec![AovDescriptor {
                name: Token::new("color"),
                format: PixelFormat::F32x4,
                clear_value: Value::Float(0.0),
                multi_sampled: false,
            }],
            ..Capabilities::default()
        };
        let container = capabilities.to_container().into_handle();
        assert_eq!(read(&container, "pause"), Some(Value::Bool(true)));
        assert_eq!(read(&container, "stop"), Some(Value::Bool(false)));
        assert_eq!(read(&container, "writesProducts"), Some(Value::Bool(true)));
        assert_eq!(
            read(&container, "materialContexts"),
            Some(Value::TokenVec(vec![Token::new("ri")]))
        );
        let aovs = locate(&container, &Locator::parse("aovs")).unwrap();
        let aov = aovs.as_vector().unwrap().get(0).unwrap();
        assert_eq!(
            read(aov.as_container().unwrap(), "format"),
            Some(Value::Token(Token::new("float32Vec4")))
        );
        assert_eq!(capabilities.default_aov().map(|a| a.name.as_str()), Some("color"));
    }
}
