//! User settings, saved as `settings.json` next to the companion save.

use serde::{Deserialize, Serialize};

pub const LANGUAGES: [&str; 3] = ["ko", "en", "ja"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// "ko" | "en" | "ja"
    pub language: String,
    /// 0 = manual refresh only.
    pub refresh_minutes: u32,
    /// "used" | "remaining"
    pub limit_display: String,
    /// Tray label/tooltip contents.
    pub tray_tokens: bool,
    pub tray_limit: bool,
    pub pet_enabled: bool,
    pub limit_notifications: bool,
    pub warn_threshold: f64,
    pub crit_threshold: f64,
    pub companion_notifications: bool,
    /// Species shown by the floating pet instead of the active Pokémon.
    pub representative: Option<u32>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: system_language(),
            refresh_minutes: 1,
            limit_display: "used".into(),
            tray_tokens: true,
            tray_limit: false,
            pet_enabled: true,
            limit_notifications: true,
            warn_threshold: 80.0,
            crit_threshold: 95.0,
            companion_notifications: true,
            representative: None,
        }
    }
}

impl Settings {
    /// Pulls hand-edited or out-of-range values back into what the UI can show.
    pub fn sanitized(mut self) -> Self {
        if !LANGUAGES.contains(&self.language.as_str()) {
            self.language = "en".into();
        }
        if self.limit_display != "remaining" {
            self.limit_display = "used".into();
        }
        self.refresh_minutes = self.refresh_minutes.min(15);
        self.warn_threshold = self.warn_threshold.clamp(50.0, 95.0);
        self.crit_threshold = self.crit_threshold.clamp(80.0, 100.0).max(self.warn_threshold);
        self
    }
}

fn system_language() -> String {
    let locale = sys_locale::get_locale().unwrap_or_default().to_lowercase();
    LANGUAGES.iter().find(|l| locale.starts_with(*l)).unwrap_or(&"en").to_string()
}

/// Notification texts. The UI has its own dictionary in `src/i18n.ts`.
pub fn text(lang: &str, key: &str) -> &'static str {
    match (lang, key) {
        ("ko", "hatched") => "알이 부화했어요!",
        ("ko", "shiny") => "✨ 이로치가 태어났어요!",
        ("ko", "evolved") => "진화했어요!",
        ("ko", "graduated") => "졸업! 도감에 등록됐어요",
        ("ko", "warn") => "한도 경고",
        ("ko", "crit") => "한도 임박",
        ("ko", "today") => "오늘",
        ("ko", "limit") => "한도",
        ("ja", "hatched") => "タマゴがかえった！",
        ("ja", "shiny") => "✨ 色違いが生まれた！",
        ("ja", "evolved") => "進化した！",
        ("ja", "graduated") => "卒業！図鑑に登録されました",
        ("ja", "warn") => "上限の警告",
        ("ja", "crit") => "上限間近",
        ("ja", "today") => "今日",
        ("ja", "limit") => "上限",
        (_, "hatched") => "Your egg hatched!",
        (_, "shiny") => "✨ A shiny hatched!",
        (_, "evolved") => "Evolved!",
        (_, "graduated") => "Graduated! Added to the Pokédex",
        (_, "warn") => "Limit warning",
        (_, "crit") => "Limit almost reached",
        (_, "today") => "today",
        (_, "limit") => "limit",
        _ => "",
    }
}

/// Alert tier for a utilization: 0 none, 1 warning, 2 critical.
pub fn alert_tier(s: &Settings, used_percent: f64) -> u8 {
    if used_percent >= s.crit_threshold {
        2
    } else if used_percent >= s.warn_threshold {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_bad_values() {
        let s = Settings {
            language: "xx".into(),
            refresh_minutes: 99,
            warn_threshold: 90.0,
            crit_threshold: 85.0,
            ..Default::default()
        }
        .sanitized();
        assert_eq!((s.language.as_str(), s.refresh_minutes, s.crit_threshold), ("en", 15, 90.0));
    }

    #[test]
    fn alert_tiers() {
        let s = Settings::default();
        assert_eq!([79.0, 80.0, 95.0].map(|p| alert_tier(&s, p)), [0, 1, 2]);
    }
}
