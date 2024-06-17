use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
	#[serde(flatten)]
	#[serde(default = "Config::default_execution_config")]
	pub execution_config: maptos_execution_util::config::Config,
}

impl Config {
	/// The default execution config
	pub fn default_execution_config() -> maptos_execution_util::config::Config {
		maptos_execution_util::config::Config::default()
	}

	/// Gets the Config from a toml file
	pub fn try_from_toml_file(path: &PathBuf) -> Result<Self, anyhow::Error> {
		let toml_str = std::fs::read_to_string(path)?;
		let config: Config = toml::from_str(toml_str.as_str())?;
		Ok(config)
	}

	/// Tries to write the Config to a toml file
	pub fn try_write_to_toml_file(&self, path: &PathBuf) -> Result<(), anyhow::Error> {
		let toml = toml::to_string(self)?;
		std::fs::write(path, toml)?;
		Ok(())
	}
}

impl Default for Config {
	fn default() -> Self {
		Self { execution_config: Config::default_execution_config() }
	}
}
