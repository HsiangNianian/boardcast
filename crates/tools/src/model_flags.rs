//! Which model a tool talks to, from the same flags in every tool that asks one:
//!
//!   --llm <url>                  a llama-server (the default is the app's own port)
//!   --deepseek                   DeepSeek, with the key from DEEPSEEK_API_KEY
//!   --remote <url> --model <m>   any OpenAI-compatible service, with the key from BOARDCAST_API_KEY
//!   --saved                      the remote model saved in the app's Settings
//!
//! A key is only ever read from the environment or the app's settings, held in memory, and never written into a
//! report: the label that names a run in its report says which service and model it was, not how it got in.

use anyhow::{bail, Context, Result};
use boardcast_agent::settings::Settings;
use boardcast_agent::Provider;

/// The environment variable that holds the key for `--remote`.
const KEY_VAR: &str = "BOARDCAST_API_KEY";

#[derive(Default)]
pub struct ModelFlags {
    local: Option<String>,
    deepseek: bool,
    remote: Option<String>,
    model: Option<String>,
    saved: bool,
}

impl ModelFlags {
    /// Take `flag` if it is one of these, with its value from `args`. False leaves it to the tool.
    pub fn take(&mut self, flag: &str, args: &mut impl Iterator<Item = String>) -> Result<bool> {
        let mut value = || args.next().with_context(|| format!("a value for {flag}"));
        match flag {
            "--llm" => self.local = Some(value()?),
            "--deepseek" => self.deepseek = true,
            "--remote" => self.remote = Some(value()?),
            "--model" => self.model = Some(value()?),
            "--saved" => self.saved = true,
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// The model chosen, with a label naming it in reports, or `None` if no flag chose one.
    pub fn chosen(self) -> Result<Option<(Provider, String)>> {
        let chosen = [self.local.is_some(), self.deepseek, self.remote.is_some(), self.saved].iter().filter(|&&c| c).count();
        if chosen > 1 {
            bail!("choose one model: --llm, --deepseek, --remote or --saved");
        }
        if self.model.is_some() && self.remote.is_none() {
            bail!("--model goes with --remote <url>");
        }
        if let Some(base) = self.local {
            return Ok(Some((Provider::Local { base }, "local".into())));
        }
        if self.deepseek {
            return Ok(Some((Provider::deepseek_from_env()?, "deepseek".into())));
        }
        if let Some(base_url) = self.remote {
            let model = self.model.context("--remote needs --model <name>")?;
            let key = std::env::var(KEY_VAR).with_context(|| format!("--remote reads its key from {KEY_VAR}, which is not set"))?;
            let label = label(&base_url, &model);
            return Ok(Some((Provider::Remote { base_url, key, model }, label)));
        }
        if self.saved {
            let s = Settings::load();
            if !s.is_complete() {
                bail!("--saved: no remote model is saved in the app's Settings (address, key and model)");
            }
            let label = label(&s.base_url, &s.model);
            return Ok(Some((Provider::Remote { base_url: s.base_url, key: s.api_key, model: s.model }, label)));
        }
        Ok(None)
    }

    /// The model chosen, or a llama-server on the app's own port when no flag chose one.
    pub fn or_local(self) -> Result<(Provider, String)> {
        Ok(self.chosen()?.unwrap_or_else(|| (Provider::Local { base: "http://127.0.0.1:18765".into() }, "local".into())))
    }
}

/// `host-model`, safe as part of a file name: which service and model ran, never the key.
fn label(base_url: &str, model: &str) -> String {
    let host = base_url.split("//").last().unwrap_or(base_url).split('/').next().unwrap_or_default();
    format!("{host}-{model}").chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags(words: &[&str]) -> Result<Option<(Provider, String)>> {
        let mut f = ModelFlags::default();
        let mut args = words.iter().map(|s| s.to_string());
        while let Some(flag) = args.next() {
            assert!(f.take(&flag, &mut args)?, "{flag} is a model flag");
        }
        f.chosen()
    }

    #[test]
    fn a_label_names_the_service_and_model_and_is_a_file_name() {
        assert_eq!(label("https://api.example.com/v1", "gpt-x/mini:2"), "api.example.com-gpt-x_mini_2");
        assert_eq!(label("http://127.0.0.1:8080", "qwen"), "127.0.0.1_8080-qwen");
    }

    #[test]
    fn one_model_at_a_time() {
        assert!(flags(&["--llm", "http://x", "--saved"]).is_err());
        assert!(flags(&["--model", "m"]).is_err(), "--model alone");
        assert!(flags(&[]).unwrap().is_none());
        assert!(matches!(flags(&["--llm", "http://x"]).unwrap(), Some((Provider::Local { .. }, l)) if l == "local"));
    }
}
