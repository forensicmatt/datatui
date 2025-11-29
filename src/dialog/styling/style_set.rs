//! StyleSet: Data structures for row/cell styling rules
use serde::{Deserialize, Serialize};
use ratatui::style::{Color, Modifier};
use crate::dialog::filter_dialog::FilterExpr;

/// Style to apply when a rule matches
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchedStyle {
    #[serde(with = "color_serde", skip_serializing_if = "Option::is_none")]
    pub fg: Option<Color>,
    #[serde(with = "color_serde", skip_serializing_if = "Option::is_none")]
    pub bg: Option<Color>,
    #[serde(with = "modifier_serde", skip_serializing_if = "Option::is_none")]
    pub modifiers: Option<Vec<Modifier>>,
}

/// Scope where the style should be applied
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopeEnum {
    Row,
    Cell,
}

/// Combines scope with style for application
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationScope {
    pub scope: ScopeEnum,
    pub style: MatchedStyle,
}

/// A single styling rule that matches rows/cells and applies styles
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleRule {
    /// Optional column scope patterns (glob patterns like "col_*", "*_id")
    /// If None or empty, the rule applies to all columns
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_scope: Option<Vec<String>>,
    /// Filter expression to evaluate against row/cell data
    pub match_expr: FilterExpr,
    /// Style to apply when the rule matches
    pub style: ApplicationScope,
}

/// A collection of style rules with metadata
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleSet {
    pub name: String,
    pub description: String,
    pub rules: Vec<StyleRule>,
}

// Custom serialization for Color
mod color_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use ratatui::style::Color;

    pub fn serialize<S>(color: &Option<Color>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match color {
            None => serializer.serialize_none(),
            Some(c) => {
                let s = match c {
                    Color::Reset => "Reset".to_string(),
                    Color::Black => "Black".to_string(),
                    Color::Red => "Red".to_string(),
                    Color::Green => "Green".to_string(),
                    Color::Yellow => "Yellow".to_string(),
                    Color::Blue => "Blue".to_string(),
                    Color::Magenta => "Magenta".to_string(),
                    Color::Cyan => "Cyan".to_string(),
                    Color::White => "White".to_string(),
                    Color::Gray => "Gray".to_string(),
                    Color::DarkGray => "DarkGray".to_string(),
                    Color::LightRed => "LightRed".to_string(),
                    Color::LightGreen => "LightGreen".to_string(),
                    Color::LightYellow => "LightYellow".to_string(),
                    Color::LightBlue => "LightBlue".to_string(),
                    Color::LightMagenta => "LightMagenta".to_string(),
                    Color::LightCyan => "LightCyan".to_string(),
                    Color::Rgb(r, g, b) => format!("rgb({},{},{})", r, g, b),
                    Color::Indexed(i) => format!("indexed({})", i),
                };
                serializer.serialize_str(&s)
            }
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Color>, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        let s: Option<String> = Option::deserialize(deserializer)?;
        match s {
            None => Ok(None),
            Some(s) => {
                let color = match s.as_str() {
                    "Reset" => Color::Reset,
                    "Black" => Color::Black,
                    "Red" => Color::Red,
                    "Green" => Color::Green,
                    "Yellow" => Color::Yellow,
                    "Blue" => Color::Blue,
                    "Magenta" => Color::Magenta,
                    "Cyan" => Color::Cyan,
                    "White" => Color::White,
                    "Gray" => Color::Gray,
                    "DarkGray" => Color::DarkGray,
                    "LightRed" => Color::LightRed,
                    "LightGreen" => Color::LightGreen,
                    "LightYellow" => Color::LightYellow,
                    "LightBlue" => Color::LightBlue,
                    "LightMagenta" => Color::LightMagenta,
                    "LightCyan" => Color::LightCyan,
                    s if s.starts_with("rgb(") && s.ends_with(")") => {
                        let inner = &s[4..s.len()-1];
                        let parts: Vec<&str> = inner.split(',').collect();
                        if parts.len() == 3 {
                            if let (Ok(r), Ok(g), Ok(b)) = (
                                parts[0].trim().parse::<u8>(),
                                parts[1].trim().parse::<u8>(),
                                parts[2].trim().parse::<u8>(),
                            ) {
                                Color::Rgb(r, g, b)
                            } else {
                                return Err(Error::custom(format!("Invalid RGB color: {}", s)));
                            }
                        } else {
                            return Err(Error::custom(format!("Invalid RGB color format: {}", s)));
                        }
                    }
                    s if s.starts_with("indexed(") && s.ends_with(")") => {
                        let inner = &s[8..s.len()-1];
                        if let Ok(idx) = inner.parse::<u8>() {
                            Color::Indexed(idx)
                        } else {
                            return Err(Error::custom(format!("Invalid indexed color: {}", s)));
                        }
                    }
                    _ => return Err(Error::custom(format!("Unknown color: {}", s))),
                };
                Ok(Some(color))
            }
        }
    }
}

// Custom serialization for Modifier
mod modifier_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use ratatui::style::Modifier;

    pub fn serialize<S>(modifiers: &Option<Vec<Modifier>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match modifiers {
            None => serializer.serialize_none(),
            Some(mods) => {
                let strings: Vec<String> = mods.iter().map(|m| {
                    match *m {
                        Modifier::BOLD => "Bold",
                        Modifier::DIM => "Dim",
                        Modifier::ITALIC => "Italic",
                        Modifier::UNDERLINED => "Underlined",
                        Modifier::SLOW_BLINK => "SlowBlink",
                        Modifier::RAPID_BLINK => "RapidBlink",
                        Modifier::REVERSED => "Reversed",
                        Modifier::HIDDEN => "Hidden",
                        Modifier::CROSSED_OUT => "CrossedOut",
                        _ => "Unknown",
                    }.to_string()
                }).collect();
                use serde::ser::SerializeSeq;
                let mut seq = serializer.serialize_seq(Some(strings.len()))?;
                for item in &strings {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Vec<Modifier>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        let strings: Option<Vec<String>> = Option::deserialize(deserializer)?;
        match strings {
            None => Ok(None),
            Some(strs) => {
                let mut mods = Vec::new();
                for s in strs {
                    match s.as_str() {
                        "Bold" => mods.push(Modifier::BOLD),
                        "Dim" => mods.push(Modifier::DIM),
                        "Italic" => mods.push(Modifier::ITALIC),
                        "Underlined" => mods.push(Modifier::UNDERLINED),
                        "SlowBlink" => mods.push(Modifier::SLOW_BLINK),
                        "RapidBlink" => mods.push(Modifier::RAPID_BLINK),
                        "Reversed" => mods.push(Modifier::REVERSED),
                        "Hidden" => mods.push(Modifier::HIDDEN),
                        "CrossedOut" => mods.push(Modifier::CROSSED_OUT),
                        _ => return Err(Error::custom(format!("Unknown modifier: {}", s))),
                    }
                }
                Ok(Some(mods))
            }
        }
    }
}

// MatchedStyle implementation
impl MatchedStyle {
    pub fn to_ratatui_style(&self) -> ratatui::style::Style {
        let mut style = ratatui::style::Style::default();
        if let Some(fg) = self.fg {
            style = style.fg(fg);
        }
        if let Some(bg) = self.bg {
            style = style.bg(bg);
        }
        if let Some(ref mods) = self.modifiers {
            for m in mods {
                style = style.add_modifier(*m);
            }
        }
        style
    }
}

/// Check if a column name matches any of the given glob patterns
pub fn matches_column(column: &str, patterns: &[String]) -> bool {
    use globset::Glob;
    
    for pattern in patterns {
        // Try to create a glob matcher
        if let Ok(glob) = Glob::new(pattern) {
            let matcher = glob.compile_matcher();
            if matcher.is_match(column) {
                return true;
            }
        }
    }
    false
}


