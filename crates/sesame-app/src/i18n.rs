//! English and Russian UI text. Strings live in `locales/app.yml`.

use std::borrow::Cow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LangPref {
    System,
    En,
    Ru,
}

impl LangPref {
    pub fn parse(s: &str) -> Self {
        match s {
            "en" => LangPref::En,
            "ru" => LangPref::Ru,
            _ => LangPref::System,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            LangPref::System => "system",
            LangPref::En => "en",
            LangPref::Ru => "ru",
        }
    }

    /// The locale code this preference resolves to right now.
    pub fn locale(self) -> &'static str {
        match self {
            LangPref::En => "en",
            LangPref::Ru => "ru",
            LangPref::System => {
                let sys = sys_locale::get_locale().unwrap_or_default().to_lowercase();
                // Russian also for the CIS locales people commonly run in Russian.
                if ["ru", "be", "kk", "ky", "uz", "az", "tg", "hy"].iter().any(|l| sys.starts_with(l)) {
                    "ru"
                } else {
                    "en"
                }
            }
        }
    }
}

pub fn apply(pref: LangPref) {
    rust_i18n::set_locale(pref.locale());
}

/// Translated text for `key`.
pub fn tr(key: &str) -> String {
    rust_i18n::t!(key).into_owned()
}

/// Translated text with `%{name}` placeholders filled in.
pub fn trp(key: &str, params: &[(&str, &str)]) -> String {
    let mut s: Cow<str> = rust_i18n::t!(key);
    for (k, v) in params {
        s = Cow::Owned(s.replace(&format!("%{{{k}}}"), v));
    }
    s.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn russian_and_english_with_params() {
        rust_i18n::set_locale("ru");
        assert_eq!(trp("insight.disk_full", &[("percent", "91")]), "Диск заполнен на 91%");
        rust_i18n::set_locale("en");
        assert_eq!(tr("nav.sites"), "Sites");
    }

    #[test]
    fn every_key_has_both_languages() {
        let yaml = include_str!("../locales/app.yml");
        for line in yaml.lines().filter(|l| l.contains(": {")) {
            assert!(line.contains(" en: ") && line.contains(" ru: "), "missing translation: {line}");
        }
    }
}
