use serde::{Deserialize, Serialize};

// Keep owned strings at the UI boundary; GPUI labels may outlive this render.
macro_rules! tr {
    ($($args:tt)*) => { rust_i18n::t!($($args)*).into_owned() };
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub enum UiLanguage {
    #[default]
    #[serde(rename = "zh-CN")]
    Chinese,
    #[serde(rename = "en")]
    English,
    #[serde(rename = "de")]
    German,
    #[serde(rename = "ja")]
    Japanese,
}

impl UiLanguage {
    pub const ALL: [Self; 4] = [Self::Chinese, Self::English, Self::German, Self::Japanese];

    pub fn code(self) -> &'static str {
        match self {
            Self::Chinese => "zh-CN",
            Self::English => "en",
            Self::German => "de",
            Self::Japanese => "ja",
        }
    }

    pub fn native_name(self) -> &'static str {
        match self {
            Self::Chinese => "简体中文",
            Self::English => "English",
            Self::German => "Deutsch",
            Self::Japanese => "日本語",
        }
    }

    pub fn from_code(code: &str) -> Self {
        match code
            .split(['-', '_'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "en" => Self::English,
            "de" => Self::German,
            "ja" => Self::Japanese,
            _ => Self::Chinese,
        }
    }

    pub fn apply(self) {
        register_component_translations();
        // GPUI Kit uses the same rust-i18n runtime for built-in controls.
        gpui_kit::component::set_locale(self.code());
    }
}

fn register_component_translations() {
    static REGISTER: std::sync::Once = std::sync::Once::new();
    REGISTER.call_once(|| {
        use gpui_kit::component as gpui_component;
        rust_i18n::extend!(gpui_component);
    });
}

/// Translate application errors at the desktop boundary; CLI diagnostics and
/// lower-level service details retain their original representation.
pub fn app_error(error: &app::Error) -> String {
    use app::{AppleError, Error};
    match error {
        Error::InvalidJobs => tr!("errors.invalid_jobs"),
        Error::SubscriptionRequired => tr!("errors.subscription"),
        Error::EmptyQueue => tr!("resolve.no_tracks"),
        Error::Worker => tr!("errors.worker"),
        Error::Apple(AppleError::TrackUnavailable) => tr!("errors.track_unavailable"),
        Error::Apple(AppleError::PlaybackRejected { code }) => {
            tr!("errors.playback_rejected", code = code)
        }
        Error::Apple(AppleError::PlaybackUnavailable) => tr!("errors.playback_unavailable"),
        Error::Apple(AppleError::LicenseRejected) => tr!("errors.license"),
        Error::Apple(AppleError::ExpiredCookies) => tr!("login.expired"),
        Error::Apple(AppleError::InvalidCookies(reason)) => tr!("errors.cookies", reason = reason),
        Error::Apple(AppleError::InvalidUrl(reason)) => tr!("errors.url", reason = reason),
        Error::Apple(AppleError::TokenUnavailable) => tr!("errors.developer_token"),
        _ => tr!("errors.details", details = error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn catalogs_have_matching_keys_and_placeholders() {
        let english: BTreeMap<String, String> =
            serde_json::from_str(include_str!("../locales/en.json")).unwrap();
        fn parameters(text: &str) -> Vec<&str> {
            let mut result: Vec<_> = text
                .split("%{")
                .skip(1)
                .map(|part| part.split('}').next().unwrap())
                .collect();
            result.sort_unstable();
            result
        }
        for (locale, source) in [
            ("zh-CN", include_str!("../locales/zh-CN.json")),
            ("de", include_str!("../locales/de.json")),
            ("ja", include_str!("../locales/ja.json")),
        ] {
            let catalog: BTreeMap<String, String> = serde_json::from_str(source).unwrap();
            assert_eq!(
                catalog.keys().collect::<Vec<_>>(),
                english.keys().collect::<Vec<_>>(),
                "{locale}"
            );
            for (key, value) in &catalog {
                assert!(!value.trim().is_empty(), "{locale}: {key}");
                assert_eq!(
                    parameters(value),
                    parameters(&english[key]),
                    "{locale}: {key}"
                );
            }
        }
    }

    #[test]
    fn english_lookup_and_interpolation_use_embedded_catalog() {
        assert_eq!(
            tr!("settings.ui_language", locale = "en"),
            "Interface language"
        );
        assert_eq!(
            tr!("selection.count", locale = "en", p0 = 2, p1 = 5),
            "Selected 2 / 5 tracks"
        );
        // A regional locale without its own catalog falls back to English.
        assert_eq!(
            tr!("settings.ui_language", locale = "en-US"),
            "Interface language"
        );
    }

    #[test]
    fn language_codes_roundtrip_without_changing_metadata_language() {
        for language in UiLanguage::ALL {
            let preferences = crate::preferences::Preferences {
                ui_language: language,
                ..Default::default()
            };
            let encoded = toml::to_string(&preferences).unwrap();
            let decoded: crate::preferences::Preferences = toml::from_str(&encoded).unwrap();
            assert_eq!(decoded.ui_language, language);
            assert_eq!(decoded.config.language, preferences.config.language);
            assert_eq!(UiLanguage::from_code(language.code()), language);
        }
        assert_eq!(UiLanguage::from_code("en-US"), UiLanguage::English);
        assert_eq!(UiLanguage::from_code("de-DE"), UiLanguage::German);
        assert_eq!(UiLanguage::from_code("DE_at"), UiLanguage::German);
        assert_eq!(UiLanguage::from_code("ja-JP"), UiLanguage::Japanese);
    }

    #[test]
    fn german_and_japanese_interpolation_and_regional_fallback() {
        assert_eq!(
            tr!("settings.ui_language", locale = "de-DE"),
            "Oberflächensprache"
        );
        assert_eq!(tr!("settings.ui_language", locale = "ja-JP"), "表示言語");
        assert_eq!(
            tr!("selection.count", locale = "de", p0 = 2, p1 = 5),
            "Ausgewählt: 2 / 5 Titel"
        );
        assert_eq!(
            tr!("selection.count", locale = "ja", p0 = 2, p1 = 5),
            "5 曲中 2 曲を選択"
        );
    }

    #[test]
    fn built_in_controls_use_application_translations() {
        register_component_translations();
        assert_eq!(
            gpui_kit::component::_rust_i18n_translate("de", "Settings.search_placeholder"),
            "Suchen…"
        );
        assert_eq!(
            gpui_kit::component::_rust_i18n_translate("ja", "Settings.search_placeholder"),
            "検索…"
        );
        assert_eq!(
            gpui_kit::component::_rust_i18n_translate("de", "Input.Copy"),
            "Kopieren"
        );
        assert_eq!(
            gpui_kit::component::_rust_i18n_translate("ja", "ComboBox.empty"),
            "該当する項目がありません"
        );
    }
}
