//! Language selection and locale-aware number/time formatting shared by the
//! core (recommendations, report) and the GUI.

use serde::{Deserialize, Serialize};

/// Interface / report language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Language {
    /// Français
    #[default]
    French,
    /// English
    English,
}

impl Language {
    /// Native name of the language.
    #[must_use]
    pub fn native_name(self) -> &'static str {
        match self {
            Self::French => "Français",
            Self::English => "English",
        }
    }

    /// BCP-47 code.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::French => "fr",
            Self::English => "en",
        }
    }

    /// Pick the French or English variant.
    #[must_use]
    pub fn pick<'a>(self, fr: &'a str, en: &'a str) -> &'a str {
        match self {
            Self::French => fr,
            Self::English => en,
        }
    }
}

/// Non-breaking space used by French typography before units and `:`.
pub const NBSP: char = '\u{a0}';

/// Format a number with `decimals` digits, using the decimal comma in French
/// and thin non-breaking spaces as thousands separator (French) or commas (English).
#[must_use]
pub fn fmt_num(value: f64, decimals: usize, lang: Language) -> String {
    if !value.is_finite() {
        return "—".to_string();
    }
    let raw = format!("{:.*}", decimals, value.abs());
    let (int_part, frac_part) = match raw.split_once('.') {
        Some((i, f)) => (i.to_string(), Some(f.to_string())),
        None => (raw.clone(), None),
    };
    let sep = match lang {
        Language::French => '\u{202f}',
        Language::English => ',',
    };
    let mut grouped = String::new();
    let len = int_part.len();
    for (i, ch) in int_part.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 && len > 4 {
            grouped.push(sep);
        }
        grouped.push(ch);
    }
    let mut out = String::new();
    if value < 0.0 && raw.chars().any(|c| c.is_ascii_digit() && c != '0') {
        out.push('-');
    }
    out.push_str(&grouped);
    if let Some(f) = frac_part {
        out.push(match lang {
            Language::French => ',',
            Language::English => '.',
        });
        out.push_str(&f);
    }
    out
}

/// Format a number followed by a unit with the right (non-breaking) space.
#[must_use]
pub fn fmt_unit(value: f64, decimals: usize, unit: &str, lang: Language) -> String {
    format!("{}{NBSP}{unit}", fmt_num(value, decimals, lang))
}

/// Format a percentage (`19,5 %` in French, `19.5%` in English).
#[must_use]
pub fn fmt_pct(value: f64, decimals: usize, lang: Language) -> String {
    match lang {
        Language::French => format!("{}{NBSP}%", fmt_num(value, decimals, lang)),
        Language::English => format!("{}%", fmt_num(value, decimals, lang)),
    }
}

/// Human-readable duration: `45 s`, `3 min 20 s`, `2 h 05 min`.
#[must_use]
pub fn fmt_duration(seconds: f64, _lang: Language) -> String {
    if !seconds.is_finite() {
        return "—".to_string();
    }
    let s = seconds.max(0.0);
    if s < 1.0 {
        return format!("{:.1}{NBSP}s", s).replace(
            '.',
            match _lang {
                Language::French => ",",
                Language::English => ".",
            },
        );
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let total = s.round() as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let sec = total % 60;
    if h > 0 {
        format!("{h}{NBSP}h{NBSP}{m:02}{NBSP}min")
    } else if m > 0 {
        format!("{m}{NBSP}min{NBSP}{sec:02}{NBSP}s")
    } else {
        format!("{sec}{NBSP}s")
    }
}

/// Compact clock format `mm:ss` or `h:mm:ss` for axes and tables.
#[must_use]
pub fn fmt_clock(seconds: f64) -> String {
    if !seconds.is_finite() {
        return "—".to_string();
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let total = seconds.max(0.0).round() as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// Escape a string for safe inclusion in HTML text or attribute values.
#[must_use]
pub fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn french_number_formatting() {
        assert_eq!(fmt_num(19.456, 1, Language::French), "19,5");
        assert_eq!(fmt_num(19.456, 1, Language::English), "19.5");
        assert_eq!(fmt_num(12345.0, 0, Language::French), "12\u{202f}345");
        assert_eq!(fmt_num(12345.0, 0, Language::English), "12,345");
        assert_eq!(fmt_num(1234.0, 0, Language::English), "1234");
        assert_eq!(fmt_num(-0.04, 1, Language::English), "0.0");
        assert_eq!(fmt_pct(19.0, 1, Language::French), "19,0\u{a0}%");
    }

    #[test]
    fn durations() {
        assert_eq!(fmt_duration(45.0, Language::French), "45\u{a0}s");
        assert_eq!(fmt_duration(200.0, Language::French), "3\u{a0}min\u{a0}20\u{a0}s");
        assert_eq!(fmt_duration(7500.0, Language::French), "2\u{a0}h\u{a0}05\u{a0}min");
        assert_eq!(fmt_clock(65.0), "01:05");
        assert_eq!(fmt_clock(3725.0), "1:02:05");
    }

    #[test]
    fn html_escaping() {
        assert_eq!(escape_html("<script>'&\""), "&lt;script&gt;&#x27;&amp;&quot;");
    }
}
