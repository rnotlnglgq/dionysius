use std::path::PathBuf;

use bevy_reflect::Reflect;
use serde::Deserialize;

use crate::handlers::toml_config::{InheritableConfig, HasInheritableConfig};
use crate::task::PushTask;

use super::toml_config::OnRecursion;

// This is a phantom type.
#[derive(Debug, Clone, Deserialize, Reflect)]
pub struct TriggerConfig {
	pub assets: Option<TriggerInheriableConfig>,
	pub heritage: Option<TriggerInheriableConfig>,
}
#[derive(Debug, Clone, Deserialize, Reflect)]
pub struct TriggerInheriableConfig {
	pub on_recursion: Option<OnRecursion>
}
#[derive(Debug)]
pub struct TriggerTask {
	pub current_dir: PathBuf
}
impl InheritableConfig for TriggerInheriableConfig {
	fn inherit_from(&self, _: &Self) -> Self{
		unreachable!()
	}
}
impl HasInheritableConfig for TriggerConfig {
	type M = TriggerInheriableConfig;
	fn get_heritage_config(&self) -> &Self::M {
		unreachable!()
	}
	fn get_assets_config(&self) -> &Self::M {
		unreachable!()
	}
	fn inherit_from(&self, _: &Self) -> Self {
		unreachable!()
	}
}
impl PushTask for TriggerTask {
	fn execute(&self, command_list: &mut Option<Vec<String>>) {
		// do nothing now.
	}
	fn exclude_pattern_options(&self) -> Vec<String> {
		unreachable!()
	}

	fn preview(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
		println!("Trigger: {:?}", self.current_dir);
		Ok(())
	}
}