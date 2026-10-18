use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    #[default]
    Browser,
    CookieFile,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    #[serde(rename = "download")]
    pub config: app::Config,
    pub dark: bool,
    pub auth_method: AuthMethod,
    pub ui_language: crate::i18n::UiLanguage,
}

pub fn path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("gamdl-rs")
        .join("desktop.toml")
}

pub async fn load(path: &Path) -> Result<Preferences, String> {
    match tokio::fs::read_to_string(path).await {
        Ok(text) => decode(&text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Preferences::default()),
        Err(error) => Err(tr!("error.preferences_read", error = error)),
    }
}

fn decode(text: &str) -> Result<Preferences, String> {
    let mut preferences: Preferences =
        toml::from_str(text).map_err(|error| tr!("error.preferences_read", error = error))?;
    // Earlier desktop builds persisted this relative default. Migrate only
    // that exact value; all other user-selected directories remain unchanged.
    if preferences.config.output_path == Path::new("Apple Music") {
        preferences.config.output_path = app::Config::default().output_path;
    }
    Ok(preferences)
}

pub async fn save(path: &Path, preferences: &Preferences) -> Result<(), String> {
    let text = toml::to_string_pretty(preferences).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }
    let temporary = path.with_extension("toml.tmp");
    tokio::fs::write(&temporary, text)
        .await
        .map_err(|e| e.to_string())?;
    if let Err(error) = tokio::fs::rename(&temporary, path).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error.to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persisted_preferences_restore_download_settings_and_theme() {
        let mut preferences = Preferences {
            dark: true,
            ..Default::default()
        };
        preferences.config.jobs = 5;
        preferences.config.cookies_path = PathBuf::from("cookies with spaces.txt");
        let encoded = toml::to_string_pretty(&preferences).unwrap();
        let decoded: Preferences = toml::from_str(&encoded).unwrap();
        assert!(decoded.dark);
        assert_eq!(decoded.config.jobs, 5);
        assert_eq!(decoded.config.cookies_path, preferences.config.cookies_path);
    }

    #[test]
    fn legacy_output_default_migrates_without_resetting_other_preferences() {
        let preferences = decode(
            "dark = true\nui_language = 'de'\n[download]\noutput_path = 'Apple Music'\njobs = 5\nlanguage = 'ja-JP'\n",
        ).unwrap();
        assert_eq!(
            preferences.config.output_path,
            app::Config::default().output_path
        );
        assert!(preferences.config.output_path.is_absolute());
        assert_eq!(preferences.config.output_path.file_name().unwrap(), "AMd");
        assert!(preferences.dark);
        assert_eq!(preferences.ui_language, crate::i18n::UiLanguage::German);
        assert_eq!(preferences.config.jobs, 5);
        assert_eq!(preferences.config.language, "ja-JP");
    }

    #[test]
    fn custom_output_directories_are_preserved() {
        for path in ["My Music", "/music/Apple Music", "D:\\Music\\Apple Music"] {
            let mut preferences = Preferences::default();
            preferences.config.output_path = path.into();
            let restored = decode(&toml::to_string(&preferences).unwrap()).unwrap();
            assert_eq!(restored.config.output_path, preferences.config.output_path);
        }
    }
}
