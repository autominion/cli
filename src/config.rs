use core::fmt;
use std::path::PathBuf;
use std::{collections::HashMap, fs};

use anyhow::anyhow;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use url::Url;

static OPENROUTER_CHAT_COMPLETIONS_URL: Lazy<Url> = Lazy::new(|| {
    Url::parse("https://openrouter.ai/api/v1/chat/completions")
        .expect("Failed to parse OpenRouter chat completions URL")
});

static GROQ_CHAT_COMPLETIONS_URL: Lazy<Url> = Lazy::new(|| {
    Url::parse("https://api.groq.com/openai/v1/chat/completions")
        .expect("Failed to parse Groq chat completions URL")
});

static GEMINI_CHAT_COMPLETIONS_URL: Lazy<Url> = Lazy::new(|| {
    Url::parse("https://generativelanguage.googleapis.com/v1beta/openai/chat/completions")
        .expect("Failed to parse Gemini chat completions URL")
});

static COHERE_CHAT_COMPLETIONS_URL: Lazy<Url> = Lazy::new(|| {
    Url::parse("https://api.cohere.ai/compatibility/v1/chat/completions")
        .expect("Failed to parse Cohere chat completions URL")
});

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Config {
    pub llm_provider: Option<LLMProvider>,
    pub openrouter_key: Option<String>,
    pub groq_key: Option<String>,
    pub google_gemini_key: Option<String>,
    pub cohere_key: Option<String>,
}

#[derive(clap::ValueEnum, Clone, Debug, Deserialize, Serialize)]
pub enum LLMProvider {
    #[serde(rename = "openrouter")]
    #[clap(name = "openrouter")]
    OpenRouter,
    #[serde(rename = "groq")]
    Groq,
    #[serde(rename = "google-gemini")]
    GoogleGemini,
    #[serde(rename = "cohere")]
    Cohere,
}

impl LLMProvider {
    pub fn tag(&self) -> &'static str {
        match self {
            LLMProvider::OpenRouter => "openrouter",
            LLMProvider::Groq => "groq",
            LLMProvider::GoogleGemini => "google-gemini",
            LLMProvider::Cohere => "cohere",
        }
    }
}

impl fmt::Display for LLMProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LLMProvider::OpenRouter => write!(f, "OpenRouter"),
            LLMProvider::Groq => write!(f, "Groq"),
            LLMProvider::GoogleGemini => write!(f, "Google Gemini"),
            LLMProvider::Cohere => write!(f, "Cohere"),
        }
    }
}

pub struct LLMRouterTable {
    pub default_provider: String,
    pub providers: HashMap<String, LLMProviderDetails>,
}

impl LLMRouterTable {
    pub fn details_for_model(&self, provider_and_model: &str) -> (String, &LLMProviderDetails) {
        provider_and_model
            .split_once('/')
            .and_then(|(provider_name, model_name)| {
                self.providers
                    .get(provider_name)
                    .map(|details| (model_name.to_owned(), details))
            })
            .unwrap_or_else(|| {
                (
                    provider_and_model.to_owned(),
                    self.providers
                        .get(&self.default_provider)
                        .expect("Default provider not found"),
                )
            })
    }
}

pub struct LLMProviderDetails {
    pub api_chat_completions_endpoint: Url,
    pub api_key: String,
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

    pub fn llm_router_table(&self) -> Option<LLMRouterTable> {
        let mut providers = HashMap::new();

        if let Some(key) = &self.openrouter_key {
            providers.insert(
                "openrouter".to_string(),
                LLMProviderDetails {
                    api_chat_completions_endpoint: OPENROUTER_CHAT_COMPLETIONS_URL.clone(),
                    api_key: key.clone(),
                },
            );
        }
        if let Some(key) = &self.groq_key {
            providers.insert(
                "groq".to_string(),
                LLMProviderDetails {
                    api_chat_completions_endpoint: GROQ_CHAT_COMPLETIONS_URL.clone(),
                    api_key: key.clone(),
                },
            );
        }
        if let Some(key) = &self.google_gemini_key {
            providers.insert(
                "google-gemini".to_string(),
                LLMProviderDetails {
                    api_chat_completions_endpoint: GEMINI_CHAT_COMPLETIONS_URL.clone(),
                    api_key: key.clone(),
                },
            );
        }
        if let Some(key) = &self.cohere_key {
            providers.insert(
                "cohere".to_string(),
                LLMProviderDetails {
                    api_chat_completions_endpoint: COHERE_CHAT_COMPLETIONS_URL.clone(),
                    api_key: key.clone(),
                },
            );
        }

        let Some(default_llm_provider) = &self.llm_provider else {
            return None;
        };

        Some(LLMRouterTable {
            default_provider: default_llm_provider.tag().to_string(),
            providers,
        })
    }

    pub fn llm_provider_details(&self) -> Option<LLMProviderDetails> {
        match self.llm_provider {
            Some(LLMProvider::OpenRouter) => {
                self.openrouter_key.as_ref().map(|key| LLMProviderDetails {
                    api_chat_completions_endpoint: OPENROUTER_CHAT_COMPLETIONS_URL.clone(),
                    api_key: key.clone(),
                })
            }
            Some(LLMProvider::Groq) => self.groq_key.as_ref().map(|key| LLMProviderDetails {
                api_chat_completions_endpoint: GROQ_CHAT_COMPLETIONS_URL.clone(),
                api_key: key.clone(),
            }),
            Some(LLMProvider::GoogleGemini) => {
                self.google_gemini_key
                    .as_ref()
                    .map(|key| LLMProviderDetails {
                        api_chat_completions_endpoint: GEMINI_CHAT_COMPLETIONS_URL.clone(),
                        api_key: key.clone(),
                    })
            }
            Some(LLMProvider::Cohere) => self.cohere_key.as_ref().map(|key| LLMProviderDetails {
                api_chat_completions_endpoint: COHERE_CHAT_COMPLETIONS_URL.clone(),
                api_key: key.clone(),
            }),
            None => None,
        }
    }
}
