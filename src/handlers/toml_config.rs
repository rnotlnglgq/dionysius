use std::fs;
use std::path::Path;
use bevy_reflect::{Reflect, Struct as _};

use serde::Deserialize;
use strum::VariantNames;
use toml::Table;

use super::{borg::BorgConfig, git::GitConfig, trigger::TriggerConfig};


// *************************************************************************** //
// Extendable Config
// *************************************************************************** //

#[derive(Debug, Deserialize, Clone, Reflect)]
pub struct DionysiusConfig {
    // pub common: Option<CommonConfig>,
    pub trigger: Option<PushTaskConfig>,
    pub git: Option<PushTaskConfig>,
    pub borg: Option<PushTaskConfig>,
    // pub ntfs: Option<NTFSConfig>,
}
// #[derive(Debug, Deserialize, Reflect)]
// pub struct DionysiusConfig {
//     // pub common: Option<CommonConfig>,
//     pub git: Option<GitConfig>,
//     pub borg: Option<BorgConfig>,
//     // pub ntfs: Option<NTFSConfig>,
// }
impl std::fmt::Display for DionysiusConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "DionysiusConfig:")?;
        // if let Some(common) = &self.common {
        //     writeln!(f, "  Common: {}", common)?;
        // }
        self.push_task_configs().iter().for_each(|(name, config)| {
            writeln!(f, "  {}: {:?}", name, config).unwrap()
        });
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Reflect)]
#[derive(strum_macros::VariantNames)]
pub enum PushTaskConfig {
    Trigger(TriggerConfig),
    Git(GitConfig),
    Borg(BorgConfig)
}
impl PushTaskConfig {
    pub fn accepted_trigger(&self) -> Vec<String> {
        match self {
            PushTaskConfig::Git(git_config) => {
                git_config
                    .as_child
                    .as_ref()
                    .unwrap()
                    .trigger_by
                    .clone()
                    .unwrap()
            },
            PushTaskConfig::Borg(borg_config) => {
                borg_config
                    .as_child
                    .as_ref()
                    .unwrap()
                    .trigger_by
                    .clone()
                    .unwrap()
            },
            PushTaskConfig::Trigger(_) => {
                vec!["git".to_string(), "borg".to_string()]
            }
        }
    }
}

// *************************************************************************** //
// Common traits
// *************************************************************************** //

impl DionysiusConfig {
    pub fn push_task_configs(&self) -> Vec<(&str, PushTaskConfig)> {
        let mut vec = Vec::new();
        for (i, value) in self.iter_fields().enumerate() {
            // println!("{}: {:?}", i, value);
            let field_name = self.name_at(i).unwrap();
            if let Some(Some(value)) = value.try_downcast_ref::<Option<PushTaskConfig>>() {
                vec.push((field_name, value.clone()));
            }
        };
        // println!("{:?}", vec);
        vec
    }
}

pub trait CompletableConfig {
    type CompletionResult;

    fn is_complete(&self) -> bool;
    fn completion(&self) -> Self::CompletionResult;
}

impl CompletableConfig for DionysiusConfig {
    type CompletionResult = Result<Self, &'static str>;

    fn is_complete(&self) -> bool {
        self.push_task_configs().iter().all(|(_, config)| {
            config.is_complete()
        })
    }
    fn completion(&self
        // , default_allow_modify: bool
    ) -> Result<Self, &'static str> {
        let mut clone: DionysiusConfig = self.clone();
        for (i, value) in self.iter_fields().enumerate() {
            if let Some(Some(value)) = value.try_downcast_ref::<Option<PushTaskConfig>>() {
                let mut_ref = clone.field_at_mut(i).unwrap().try_downcast_mut::<Option<PushTaskConfig>>().unwrap();
                *mut_ref = Some(value.completion()?);
            }
        };
        Ok(clone)
    }
}

impl CompletableConfig for PushTaskConfig {
    type CompletionResult = Result<Self, &'static str>;

    fn is_complete(&self) -> bool {
        match self {
            PushTaskConfig::Trigger(_) => {
                true
                // TODO
            },
            PushTaskConfig::Git(git_config) => {
                git_config.is_complete()
            },
            PushTaskConfig::Borg(borg_config) => {
                borg_config.is_complete()
            }
        }
    }
    fn completion(&self
        // , default_allow_modify: bool
    ) -> Result<Self, &'static str> {
        Ok(
            match self {
                PushTaskConfig::Trigger(t) => {
                    self.clone()
                },
                PushTaskConfig::Git(git_config) => {
                    Self::Git(git_config.completion()?)
                },
                PushTaskConfig::Borg(borg_config) => {
                    Self::Borg(borg_config.completion()?)
                },
            }
        )
    }
}

#[derive(Clone, Debug, Deserialize, Reflect)]
pub enum OnRecursion {
	#[serde(rename = "skip")]
	Skip, // regarded as nonexistent
	#[serde(rename = "include")]
	Include,
	#[serde(rename = "standalone")]
	Standalone,
	#[serde(rename = "double")]
	Double,
    #[serde(rename = "inherit")]
    Inherit,
}

pub trait InheritableConfig {
    fn inherit_from(&self, other: &Self) -> Self;
}

pub trait HasInheritableConfig {
    type M: InheritableConfig;

    fn get_config_as_super(&self) -> &Self::M;
    fn get_config_as_child(&self) -> &Self::M;
    fn inherit_from(&self, super_config: &Self) -> Self;
}

// impl fmt::Display for CommonConfig {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         writeln!(f, "CommonConfig:")?;
//         writeln!(f, "  Default Push: {:?}", self.default_push)?;
//         writeln!(f, "  Ignore: {:?}", self.ignore)?;
//         writeln!(f, "  Ignore List: {:?}", self.ignore_list)?;
//         writeln!(f, "  POSIX ACL: {:?}", self.posix_acl)?;
//         writeln!(f, "  Numeric Owner: {:?}", self.numeric_owner)
//     }
// }

// impl fmt::Display for NTFSConfig {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         writeln!(f, "NTFSConfig:")?;
//         writeln!(f, "  POSIX ACL: {:?}", self.posix_acl)?;
//         writeln!(f, "  Numeric Owner: {:?}", self.numeric_owner)
//     }
// }

// #[derive(Debug, Deserialize, Reflect)]
// pub struct CommonConfig {
//     pub default_push: Option<Vec<String>>,
//     pub ignore: Option<String>,
//     pub ignore_list: Option<Vec<String>>,
//     pub posix_acl: Option<bool>,
//     pub numeric_owner: Option<bool>,
// }

// #[derive(Debug, Deserialize, Reflect)]
// pub struct NTFSConfig {
//     pub posix_acl: Option<bool>,
//     pub numeric_owner: Option<bool>,
// }

pub fn handle_dionysius_toml(dir: &Path) {
    println!("Found dionysius.toml in: {}", dir.display());
    todo!("Handle dionysius.toml file");
}

// *************************************************************************** //
// Functions
// *************************************************************************** //

pub fn load_raw_config(file_path: &Path) -> Result<DionysiusConfig, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(file_path)?;
    // let mut config: DionysiusConfig = toml::from_str(&content)?;
    let mut toml_value: toml::Value = toml::from_str(&content)?;
    toml_value = capsulate_push_task_config(toml_value);
    let config: DionysiusConfig = toml_value.try_into()?;
    // let allow_modify = config.allow_modify.unwrap_or(false);
    // config.completion(allow_modify);
    
    // println!("{}", &config);
    if config.is_complete() {
        Ok(config)
    } else {
        let completed = config.completion();
        match completed {
            Ok(completed) => Ok(completed),
            Err(_) => Err("Config is still not complete after trying completion.".into())
        }
    }
}

pub fn load_config(file_path: &Path) -> Result<DionysiusConfig, Box<dyn std::error::Error>> {
    load_raw_config(file_path)
}

pub fn load_config_for_dir(dir: &Path) -> Result<DionysiusConfig, Box<dyn std::error::Error>> {
    let file_path = dir.join("dionysius.toml");
    load_config(&file_path)
}

pub fn capsulate_push_task_config(mut toml_value: toml::Value) -> toml::Value {
    // let content = fs::read_to_string(file_path).unwrap();
    // let mut config: toml::Value = toml::from_str(&content).unwrap();
    // println!("{:?}", toml);
    match toml_value {
        toml::Value::Table(ref mut table) => {
            for (key, value) in table.iter_mut() {
                let mut capital_key = key.clone();
                capital_key.get_mut(0..1)
                    .unwrap()
                    .make_ascii_uppercase();
                let is_push_task_config_head = PushTaskConfig::VARIANTS
                    .iter()
                    // .map(|s| s.to_string().to_lowercase())
                    .any(|s| capital_key.as_str() == *s);
                if is_push_task_config_head {
                    let mut inner: Table = Table::new();
                    inner.insert(capital_key, value.clone());
                    *value = toml::Value::Table(inner);
                }
            }
        },
        _ => {
            eprintln!("Not a table");
        }
        
    }
    toml_value
    // println!("{:?}", toml);
}
