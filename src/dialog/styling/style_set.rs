//! StyleSet: Data structures for row/cell styling rules
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ratatui::style::{Color, Modifier};
use crate::dialog::filter_dialog::FilterExpr;

// =============================================================================
// Core Style Types
// =============================================================================

/// Style to apply when a rule matches
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchedStyle {
    // `default` ensures missing fields deserialize to None when using the custom
    // serde helpers, avoiding “missing field” errors for sparse style mappings.
    #[serde(default, with = "color_serde", skip_serializing_if = "Option::is_none")]
    pub fg: Option<Color>,
    #[serde(default, with = "color_serde", skip_serializing_if = "Option::is_none")]
    pub bg: Option<Color>,
    #[serde(default, with = "modifier_serde", skip_serializing_if = "Option::is_none")]
    pub modifiers: Option<Vec<Modifier>>,
}

impl Default for MatchedStyle {
    fn default() -> Self {
        Self {
            fg: None,
            bg: None,
            modifiers: None,
        }
    }
}

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

// =============================================================================
// Regex Capture for Partial Cell Styling
// =============================================================================

/// Identifies which part of a regex match to style
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrepCapture {
    /// Apply style to the section of text that matches this named capture group
    Name(String),
    /// Apply style to the section of text that matches this numbered capture group (0 = entire match)
    Group(usize),
}

impl Default for GrepCapture {
    fn default() -> Self {
        Self::Group(0) // Entire match by default
    }
}

// =============================================================================
// Application Scope - Where to Apply Styles
// =============================================================================

/// Defines where a style should be applied
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApplicationScope {
    /// Apply style to the entire row
    Row,
    /// Apply style to the entire cell
    Cell,
    /// Apply style only to the matching regex capture group within a cell
    RegexGroup(GrepCapture),
}

impl Default for ApplicationScope {
    fn default() -> Self {
        Self::Row
    }
}

impl ApplicationScope {
    /// Get the next scope in the cycle (for UI toggling)
    pub fn next(&self) -> Self {
        match self {
            Self::Row => Self::Cell,
            Self::Cell => Self::RegexGroup(GrepCapture::default()),
            Self::RegexGroup(_) => Self::Row,
        }
    }
    
    /// Get the previous scope in the cycle (for UI toggling)
    pub fn prev(&self) -> Self {
        match self {
            Self::Row => Self::RegexGroup(GrepCapture::default()),
            Self::Cell => Self::Row,
            Self::RegexGroup(_) => Self::Cell,
        }
    }
    
    /// Display name for UI
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Row => "Row",
            Self::Cell => "Cell",
            Self::RegexGroup(_) => "Regex Group",
        }
    }
}

// =============================================================================
// Style Application - What and How to Style
// =============================================================================

/// Combines a scope with a style
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyleApplication {
    /// Where to apply the style
    #[serde(default)]
    pub scope: ApplicationScope,
    /// The style to apply
    pub style: MatchedStyle,
    /// Optional: specific columns to target (for Cell scope)
    /// If None, applies to columns that matched the condition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_columns: Option<Vec<String>>,
}

impl Default for StyleApplication {
    fn default() -> Self {
        Self {
            scope: ApplicationScope::Row,
            style: MatchedStyle::default(),
            target_columns: None,
        }
    }
}

// =============================================================================
// Conditions - When to Apply Styles
// =============================================================================

/// Defines when a style should be applied
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Condition {
    /// Match based on a filter expression (complex conditions)
    Filter {
        /// The filter expression to evaluate
        expr: FilterExpr,
        /// Columns to evaluate the filter against (glob patterns)
        /// If None or empty, evaluates against all columns
        #[serde(skip_serializing_if = "Option::is_none")]
        columns: Option<Vec<String>>,
    },
    /// Match based on a regex pattern
    Regex {
        /// The regex pattern to match
        pattern: String,
        /// Columns to match against (glob patterns)
        /// If None or empty, matches against all columns
        #[serde(skip_serializing_if = "Option::is_none")]
        columns: Option<Vec<String>>,
    },
}

impl Default for Condition {
    fn default() -> Self {
        Self::Filter {
            expr: FilterExpr::And(vec![]),
            columns: None,
        }
    }
}

// =============================================================================
// Conditional Style - Links Conditions to Style Applications
// =============================================================================

/// A condition with one or more style applications
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConditionalStyle {
    /// The condition that must match
    pub condition: Condition,
    /// Style applications to apply when the condition matches
    /// Multiple applications allow styling different scopes from one condition
    pub applications: Vec<StyleApplication>,
}

impl Default for ConditionalStyle {
    fn default() -> Self {
        Self {
            condition: Condition::default(),
            applications: vec![StyleApplication::default()],
        }
    }
}

// =============================================================================
// Dynamic Styles - Gradient and Categorical
// =============================================================================

/// Scale type for gradient interpolation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GradientScale {
    #[default]
    Linear,
    Logarithmic,
    Percentile,
}

impl GradientScale {
    pub fn next(&self) -> Self {
        match self {
            Self::Linear => Self::Logarithmic,
            Self::Logarithmic => Self::Percentile,
            Self::Percentile => Self::Linear,
        }
    }
    
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::Logarithmic => "Logarithmic",
            Self::Percentile => "Percentile",
        }
    }
}

/// Gradient style for numeric data visualization
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradientStyle {
    /// Column to read numeric values from
    pub source_column: String,
    /// Style at minimum value
    pub min_style: MatchedStyle,
    /// Style at maximum value
    pub max_style: MatchedStyle,
    /// Interpolation scale
    #[serde(default)]
    pub scale: GradientScale,
    /// Optional fixed bounds (min, max). If None, auto-detect from data
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<(f64, f64)>,
    /// Which columns to apply the gradient to (if None, applies to source_column)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_columns: Option<Vec<String>>,
}

impl Default for GradientStyle {
    fn default() -> Self {
        Self {
            source_column: String::new(),
            min_style: MatchedStyle {
                fg: None,
                bg: Some(Color::Rgb(50, 50, 150)),
                modifiers: None,
            },
            max_style: MatchedStyle {
                fg: None,
                bg: Some(Color::Rgb(150, 50, 50)),
                modifiers: None,
            },
            scale: GradientScale::Linear,
            bounds: None,
            target_columns: None,
        }
    }
}

impl GradientStyle {
    /// Interpolate between min and max styles based on a normalized value (0.0 to 1.0)
    pub fn interpolate(&self, normalized: f64) -> MatchedStyle {
        let t = normalized.clamp(0.0, 1.0);
        
        let fg = match (self.min_style.fg, self.max_style.fg) {
            (Some(min_c), Some(max_c)) => Some(interpolate_color(min_c, max_c, t)),
            (Some(c), None) | (None, Some(c)) => Some(c),
            (None, None) => None,
        };
        
        let bg = match (self.min_style.bg, self.max_style.bg) {
            (Some(min_c), Some(max_c)) => Some(interpolate_color(min_c, max_c, t)),
            (Some(c), None) | (None, Some(c)) => Some(c),
            (None, None) => None,
        };
        
        let modifiers = if t >= 0.5 {
            self.max_style.modifiers.clone()
        } else {
            self.min_style.modifiers.clone()
        };
        
        MatchedStyle { fg, bg, modifiers }
    }
    
    /// Calculate normalized value based on scale
    pub fn normalize(&self, value: f64, min: f64, max: f64) -> f64 {
        if (max - min).abs() < f64::EPSILON {
            return 0.5;
        }
        
        match self.scale {
            GradientScale::Linear => (value - min) / (max - min),
            GradientScale::Logarithmic => {
                let safe_value = value.max(0.0001);
                let safe_min = min.max(0.0001);
                let safe_max = max.max(0.0001);
                (safe_value.ln() - safe_min.ln()) / (safe_max.ln() - safe_min.ln())
            }
            GradientScale::Percentile => (value - min) / (max - min),
        }
    }
}

/// Interpolate between two colors
fn interpolate_color(c1: Color, c2: Color, t: f64) -> Color {
    match (c1, c2) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let r = ((1.0 - t) * r1 as f64 + t * r2 as f64) as u8;
            let g = ((1.0 - t) * g1 as f64 + t * g2 as f64) as u8;
            let b = ((1.0 - t) * b1 as f64 + t * b2 as f64) as u8;
            Color::Rgb(r, g, b)
        }
        _ => if t < 0.5 { c1 } else { c2 }
    }
}

/// Categorical style for auto-assigning colors to unique values
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CategoricalStyle {
    /// Column to read category values from
    pub source_column: String,
    /// Color palette to cycle through
    #[serde(with = "color_vec_serde")]
    pub palette: Vec<Color>,
    /// Apply to foreground (true) or background (false)
    #[serde(default = "default_true")]
    pub apply_to_fg: bool,
    /// Which columns to apply the categorical style to (if None, applies to source_column)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_columns: Option<Vec<String>>,
}

fn default_true() -> bool { true }

impl Default for CategoricalStyle {
    fn default() -> Self {
        Self {
            source_column: String::new(),
            palette: vec![
                Color::Rgb(66, 133, 244),   // Blue
                Color::Rgb(234, 67, 53),    // Red
                Color::Rgb(251, 188, 5),    // Yellow
                Color::Rgb(52, 168, 83),    // Green
                Color::Rgb(154, 160, 166),  // Gray
                Color::Rgb(255, 112, 67),   // Deep Orange
                Color::Rgb(156, 39, 176),   // Purple
                Color::Rgb(0, 188, 212),    // Cyan
            ],
            apply_to_fg: true,
            target_columns: None,
        }
    }
}

impl CategoricalStyle {
    pub fn get_color_for_value(&self, value: &str) -> Option<Color> {
        if self.palette.is_empty() {
            return None;
        }
        let hash: usize = value.bytes().fold(0, |acc, b| acc.wrapping_add(b as usize));
        Some(self.palette[hash % self.palette.len()])
    }
    
    pub fn get_style_for_value(&self, value: &str) -> MatchedStyle {
        let color = self.get_color_for_value(value);
        if self.apply_to_fg {
            MatchedStyle { fg: color, bg: None, modifiers: None }
        } else {
            MatchedStyle { fg: None, bg: color, modifiers: None }
        }
    }
}

// =============================================================================
// Style Logic - The Main Logic Types
// =============================================================================

/// The main logic type for a style rule
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StyleLogic {
    /// Condition-based styling with one or more style applications
    Conditional(ConditionalStyle),
    /// Gradient based on numeric column value
    Gradient(GradientStyle),
    /// Categorical palette based on unique values
    Categorical(CategoricalStyle),
}

impl Default for StyleLogic {
    fn default() -> Self {
        Self::Conditional(ConditionalStyle::default())
    }
}

impl StyleLogic {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Conditional(_) => "Conditional",
            Self::Gradient(_) => "Gradient",
            Self::Categorical(_) => "Categorical",
        }
    }
}

// =============================================================================
// Merge Mode - How to Combine Styles
// =============================================================================

/// How to combine styles when multiple rules match
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MergeMode {
    /// Replace previous style completely (default)
    #[default]
    Override,
    /// Only override non-None properties from this rule
    Merge,
    /// Add modifiers to existing, keep colors from higher priority
    Additive,
}

// =============================================================================
// Style Rule - The Top-Level Rule
// =============================================================================

/// A single styling rule
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyleRule {
    /// Optional name for the rule
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The logic that controls matching and styling
    pub logic: StyleLogic,
    /// Rule priority: higher values are processed later and win conflicts
    #[serde(default)]
    pub priority: i32,
    /// How to combine with previously matched styles
    #[serde(default)]
    pub merge_mode: MergeMode,
}

impl Default for StyleRule {
    fn default() -> Self {
        Self {
            name: None,
            logic: StyleLogic::default(),
            priority: 0,
            merge_mode: MergeMode::Override,
        }
    }
}

impl StyleRule {
    /// Create a new conditional style rule
    pub fn conditional(condition: Condition, applications: Vec<StyleApplication>) -> Self {
        Self {
            name: None,
            logic: StyleLogic::Conditional(ConditionalStyle { condition, applications }),
            priority: 0,
            merge_mode: MergeMode::Override,
        }
    }
    
    /// Create a new gradient style rule
    pub fn gradient(gradient: GradientStyle) -> Self {
        Self {
            name: None,
            logic: StyleLogic::Gradient(gradient),
            priority: 0,
            merge_mode: MergeMode::Override,
        }
    }
    
    /// Create a new categorical style rule
    pub fn categorical(categorical: CategoricalStyle) -> Self {
        Self {
            name: None,
            logic: StyleLogic::Categorical(categorical),
            priority: 0,
            merge_mode: MergeMode::Override,
        }
    }
    
    /// Set the rule name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
    
    /// Set the priority
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
    
    /// Set the merge mode
    pub fn with_merge_mode(mut self, merge_mode: MergeMode) -> Self {
        self.merge_mode = merge_mode;
        self
    }
}

// =============================================================================
// Schema Hints - For Auto-Detection
// =============================================================================

/// Expected data type for column matching
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ExpectedType {
    #[default]
    Any,
    Numeric,
    String,
    DateTime,
    Boolean,
}

/// Column matcher for schema hints
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColumnMatcher {
    ExactName(String),
    Pattern(String),
    TypedPattern { pattern: String, expected_type: ExpectedType },
}

impl ColumnMatcher {
    pub fn matches(&self, column_name: &str) -> bool {
        match self {
            Self::ExactName(name) => column_name == name,
            Self::Pattern(pattern) => matches_column(column_name, &[pattern.clone()]),
            Self::TypedPattern { pattern, .. } => matches_column(column_name, &[pattern.clone()]),
        }
    }
    
    pub fn pattern_string(&self) -> &str {
        match self {
            Self::ExactName(name) => name,
            Self::Pattern(pattern) => pattern,
            Self::TypedPattern { pattern, .. } => pattern,
        }
    }
}

/// Schema hint for auto-detection of matching datasets
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaHint {
    #[serde(default)]
    pub required_columns: Vec<ColumnMatcher>,
    #[serde(default)]
    pub optional_columns: Vec<ColumnMatcher>,
    #[serde(default = "default_min_confidence")]
    pub min_confidence: f32,
}

fn default_min_confidence() -> f32 { 0.5 }

impl Default for SchemaHint {
    fn default() -> Self {
        Self {
            required_columns: vec![],
            optional_columns: vec![],
            min_confidence: 0.5,
        }
    }
}

impl SchemaHint {
    pub fn calculate_confidence(&self, columns: &[String]) -> (f32, usize, usize, usize, usize) {
        let mut matched_required = 0;
        let mut matched_optional = 0;
        
        for matcher in &self.required_columns {
            if columns.iter().any(|c| matcher.matches(c)) {
                matched_required += 1;
            }
        }
        
        for matcher in &self.optional_columns {
            if columns.iter().any(|c| matcher.matches(c)) {
                matched_optional += 1;
            }
        }
        
        let total_required = self.required_columns.len();
        let total_optional = self.optional_columns.len();
        
        if total_required == 0 && total_optional == 0 {
            return (0.0, 0, 0, 0, 0);
        }
        
        if total_required > 0 && matched_required < total_required {
            return (0.0, matched_required, total_required, matched_optional, total_optional);
        }
        
        let required_score = if total_required > 0 {
            matched_required as f32 / total_required as f32
        } else { 0.0 };
        
        let optional_score = if total_optional > 0 {
            matched_optional as f32 / total_optional as f32
        } else { 0.0 };
        
        let score = if total_optional > 0 {
            0.7 * required_score + 0.3 * optional_score
        } else {
            required_score
        };
        
        (score, matched_required, total_required, matched_optional, total_optional)
    }
    
    pub fn matches(&self, columns: &[String]) -> bool {
        let (score, _, _, _, _) = self.calculate_confidence(columns);
        score >= self.min_confidence
    }
}

// =============================================================================
// Style Set - Collection of Rules
// =============================================================================

/// A collection of style rules with metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyleSet {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categories: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yaml_path: Option<PathBuf>,
    pub rules: Vec<StyleRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_hint: Option<SchemaHint>,
}

impl Default for StyleSet {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            categories: None,
            tags: None,
            description: String::new(),
            yaml_path: None,
            rules: vec![],
            schema_hint: None,
        }
    }
}

impl StyleSet {
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_categories(mut self, categories: Option<Vec<String>>) -> Self {
        self.categories = categories;
        self
    }

    pub fn with_tags(mut self, tags: Option<Vec<String>>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_yaml_path(mut self, yaml_path: Option<PathBuf>) -> Self {
        self.yaml_path = yaml_path;
        self
    }

    pub fn with_rules(mut self, rules: Vec<StyleRule>) -> Self {
        self.rules = rules;
        self
    }
    
    pub fn with_schema_hint(mut self, schema_hint: Option<SchemaHint>) -> Self {
        self.schema_hint = schema_hint;
        self
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Check if a column name matches any of the given glob patterns
pub fn matches_column(column: &str, patterns: &[String]) -> bool {
    use globset::Glob;
    
    for pattern in patterns {
        if let Ok(glob) = Glob::new(pattern) {
            let matcher = glob.compile_matcher();
            if matcher.is_match(column) {
                return true;
            }
        }
    }
    false
}

// =============================================================================
// Serde Modules for Color and Modifier
// =============================================================================

mod color_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use ratatui::style::Color;

    pub fn serialize<S>(color: &Option<Color>, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
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
    where D: Deserializer<'de> {
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

mod modifier_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use ratatui::style::Modifier;

    pub fn serialize<S>(modifiers: &Option<Vec<Modifier>>, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
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
    where D: Deserializer<'de> {
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

mod color_vec_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use ratatui::style::Color;
    
    pub fn serialize<S>(colors: &Vec<Color>, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        use serde::ser::SerializeSeq;
        let strings: Vec<String> = colors.iter().map(|c| {
            match c {
                Color::Rgb(r, g, b) => format!("rgb({},{},{})", r, g, b),
                Color::Black => "Black".to_string(),
                Color::Red => "Red".to_string(),
                Color::Green => "Green".to_string(),
                Color::Yellow => "Yellow".to_string(),
                Color::Blue => "Blue".to_string(),
                Color::Magenta => "Magenta".to_string(),
                Color::Cyan => "Cyan".to_string(),
                Color::White => "White".to_string(),
                _ => format!("{:?}", c),
            }
        }).collect();
        let mut seq = serializer.serialize_seq(Some(strings.len()))?;
        for s in &strings {
            seq.serialize_element(s)?;
        }
        seq.end()
    }
    
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Color>, D::Error>
    where D: Deserializer<'de> {
        use serde::de::Error;
        let strings: Vec<String> = Vec::deserialize(deserializer)?;
        let mut colors = Vec::new();
        for s in strings {
            let color = match s.as_str() {
                "Black" => Color::Black,
                "Red" => Color::Red,
                "Green" => Color::Green,
                "Yellow" => Color::Yellow,
                "Blue" => Color::Blue,
                "Magenta" => Color::Magenta,
                "Cyan" => Color::Cyan,
                "White" => Color::White,
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
                        return Err(Error::custom(format!("Invalid RGB format: {}", s)));
                    }
                }
                _ => return Err(Error::custom(format!("Unknown color: {}", s))),
            };
            colors.push(color);
        }
        Ok(colors)
    }
}
