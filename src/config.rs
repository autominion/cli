use std::fs;
use std::path::PathBuf;

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Config {
    pub openrouter_key: Option<String>,
}

impl Config {
    pub fn load_or_create() -> anyhow::Result<Self> {
        match Self::load() {
            Ok(config) => Ok(config),
            Err(_) => {
                let config = Self::default();
                config.save()?;
                Ok(config)
            }
        }
    }

    pub fn load() -> anyhow::Result<Self> {
        let text = fs::read_to_string(Self::filepath()?)?;
        let config = toml::from_str(&text)?;
        Ok(config)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let text = toml::to_string(self)?;
        fs::create_dir_all(Self::filepath()?.parent().unwrap())?;
        fs::write(Self::filepath()?, text)?;
        Ok(())
    }

    pub fn filepath() -> anyhow::Result<PathBuf> {
        Ok(dirs::config_dir()
            .ok_or(anyhow!("Failed to locate appropriate config directory"))?
            .join("minion")
            .join("config.toml"))
    }
}
