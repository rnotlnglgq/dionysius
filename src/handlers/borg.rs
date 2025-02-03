use std::{path::PathBuf, process::Command};
use bevy_reflect::Reflect;
use serde::Deserialize;

use crate::task::PushTask;

use super::{exclude::BorgPattern, toml_config::{CompletableConfig, HasInheritableConfig, InheritableConfig, OnRecursion}};

// *************************************************************************** //
// Configuration Types and Implementations
// *************************************************************************** //

#[derive(Debug, Deserialize, Clone, Reflect)]
pub struct BorgTargetConfig {
    pub mode: Option<String>,
    pub target: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Reflect)]
pub struct BorgConfig {
    pub target: Option<BorgTargetConfig>,
    pub as_child: Option<BorgInheritableConfig>,
    pub as_super: Option<BorgInheritableConfig>,
}

impl Default for BorgConfig {
    fn default() -> Self {
        BorgConfig {
            target: None,
            as_child: Some(
                BorgInheritableConfig {
                    trigger_by: vec!["borg".to_string()].into(),
                    extra_exclude_mode: vec!["git".to_string()].into(),
                    on_recursion: Some(OnRecursion::Inherit),
                    ignore_child: None,
                }
            ),
            as_super: Some(
                BorgInheritableConfig {
                    trigger_by: None, // temporarily this cannot be inherited.
                    extra_exclude_mode: None,
                    on_recursion: Some(OnRecursion::Inherit),
                    ignore_child: Some(false),
                }
            ),
        }
    }
}

impl HasInheritableConfig for BorgConfig {
	type M = BorgInheritableConfig;

	fn get_config_as_super(&self) -> &BorgInheritableConfig {
		self.as_super.as_ref().unwrap()
	}
	fn get_config_as_child(&self) -> &BorgInheritableConfig {
		self.as_child.as_ref().unwrap()
	}

    fn inherit_from(&self, super_config: &Self) -> Self {
        let mut this = self.clone();
        this.as_child = Some(
            this.as_child
                .unwrap()
                .inherit_from(
                    super_config.get_config_as_super()
                )
        );
        this
    }
}


#[derive(Debug, Deserialize, Clone, Reflect)]
pub struct BorgInheritableConfig {
    pub trigger_by: Option<Vec<String>>,
    pub extra_exclude_mode: Option<Vec<String>>,
    pub on_recursion: Option<OnRecursion>,
    pub ignore_child: Option<bool>,
}

impl InheritableConfig for BorgInheritableConfig {
    fn inherit_from(&self, super_config: &Self) -> Self {
        let mut this = self.clone();
        match self.on_recursion {
            Some(OnRecursion::Inherit) => {
                this.on_recursion = super_config.on_recursion.clone();
            },
            None => unreachable!(),
            _ => {}
        }
        this
    }
}

macro_rules! check_fields {
    ($obj:expr, $($field:ident),+) => {
        {
            let mut complete = true;
            $(
                if $obj.$field.is_none() {
                    complete = false;
                }
            )+
            complete
        }
    };
}

impl CompletableConfig for BorgConfig {
    type CompletionResult = Result<Self, &'static str>;

    fn is_complete(&self) -> bool {
        // todo: check target
        if let Some(as_child) = &self.as_child {
            if !check_fields!(as_child, trigger_by, on_recursion) {
                return false;
            }
        } else {
            return false;
        }
        if let Some(as_super) = &self.as_super {
            if !check_fields!(as_super, ignore_child, on_recursion) {
                return false;
            }
        } else {
            return false;
        }
        true
    }

    fn completion(&self) -> Self::CompletionResult {
        let mut result = self.clone();
        let default = BorgConfig::default();

        // check target
        if let Some(target) = &self.target {
            if target.target.is_none() {
                return Err("Target cannot be None");
            }
            if !["path"].contains(&target.mode.as_ref().unwrap().as_str()) {
                return Err("Invalid target mode");
            }
        } else {
            return Err("Target config is required");
        }

        // complete as_child
        if let Some(as_child) = &mut result.as_child {
            if as_child.trigger_by.is_none() {
                as_child.trigger_by = default.as_child.as_ref().unwrap().trigger_by.clone();
            }
            if as_child.on_recursion.is_none() {
                as_child.on_recursion = default.as_child.as_ref().unwrap().on_recursion.clone();
            }
        } else {
            result.as_child = default.as_child;
        }

        // complete as_super 
        if let Some(as_super) = &mut result.as_super {
            if as_super.ignore_child.is_none() {
                as_super.ignore_child = default.as_super.as_ref().unwrap().ignore_child;
            }
            if as_super.on_recursion.is_none() {
                as_super.on_recursion = default.as_super.as_ref().unwrap().on_recursion.clone();
            }
        } else {
            result.as_super = default.as_super;
        }

        Ok(result)
    }
}

// *************************************************************************** //
// Task Types and Implementations 
// *************************************************************************** //

#[derive(Debug)]
pub struct BorgCreateTask {
	pub source: PathBuf,
	pub target: String,
	pub exclude_list: Vec<PathBuf>,
    pub extra_exclude_patterns: Vec<BorgPattern>, // 新增字段
	pub options: BorgCreateOptions
}

impl BorgCreateTask {
    fn borg_exclude_patterns(&self) -> Result<Vec<BorgPattern>, &'static str> {
        let mut patterns = Vec::new();
        
        let base_path = self.source.clone();
        for exclude_path in self.exclude_list.clone() {
            if let Ok(relative) = exclude_path.strip_prefix(&base_path) {
                let pattern = relative
                    .to_string_lossy()
                    .replace('\\', "/");
                // println!("{:?}", exclude_path);
                patterns.push(BorgPattern::PathFullMatch(pattern.to_string()));
            } else {
                return Err("Exclude path must be under source path");
            }
        }

        patterns.extend(self.extra_exclude_patterns.clone());
        
        Ok(patterns)
    }
}

impl PushTask for BorgCreateTask {
    fn execute(&self, command_list: &mut Option<Vec<String>>) {
        let mut command = Command::new("borg");
        command
            .arg("create")
            .arg("--stats")
            .arg("--progress")
            .arg("--one-file-system")
            .arg("--compression")
            .arg(&self.options.compression);

        command.args(self.exclude_pattern_options());

        command
            .arg(&self.target)
            .arg(&self.source);

        if let Some(command_list) = command_list {
            command_list.push(format!("{:?}", command));
            return;
        } else {
            command.output().expect("failed to execute process");
        }
    }

    fn exclude_pattern_options(&self) -> Vec<String> {
        let mut vec: Vec<String> = Vec::new();
        let values = self
            .borg_exclude_patterns()
            .unwrap()
            .into_iter()
            .map(|p| {
                // println!("{:?}", p);
                format!("{}", p)
            });
        values.for_each(|val| {
            vec.push("--exclude".to_string());
            vec.push(val);
        });
        vec
        // maybe use a temp file if too long.
    }

    fn preview(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // these commented should be `pretend`

        // let mut command = Command::new("borg");
        // command
        //     .arg("create")
        //     .arg("--stats")
        //     .arg("--progress")
        //     .arg("--one-file-system")
        //     .arg("--dry-run")
        //     .arg("--compression")
        //     .arg(&self.options.compression);

        // command.args(self.exclude_pattern_options());

        // command
        //     .arg(&self.target)
        //     .arg(&self.source);

        // command.output().expect("failed to execute process");
        println!(
            "Borg archive: [{}] -> [{}]", 
            self.source.canonicalize()?.display(),
            self.target
        );
        Ok(())
    }
}

#[derive(Debug)]
pub struct BorgCreateOptions {
	pub acl: bool,
	pub numeric_owner: bool,
	pub compression: String,
}

impl Default for BorgCreateOptions {
    fn default() -> Self {
        BorgCreateOptions {
            acl: true,
            numeric_owner: true,
            compression: "zstd".to_string(),
        }
    }
}

// *************************************************************************** //
// Error Types
// *************************************************************************** //