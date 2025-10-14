#![allow(dead_code)] // Remove this once you start using the code

use std::{collections::HashMap, env, fs, path::PathBuf};

use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use derive_deref::{Deref, DerefMut};
use lazy_static::lazy_static;
use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize, de::Deserializer};
 
use directories::BaseDirs;

use crate::action::Action;

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    DataTabManager,
    Global,
    DataTableContainer,
    DataManagement,
    DataImport,
    CsvOptions,
    Sort,
    Filter,
    Find,
    FindAllResults,
    JmesPath,
    SqlDialog,
    XlsxOptionsDialog,
    ParquetOptionsDialog,
    SqliteOptionsDialog,
    FileBrowser,
    ColumnWidthDialog,
    JsonOptionsDialog,
    AliasEdit,
    ColumnOperationOptions,
    ColumnOperations,
    DataFrameDetails,
    MessageDialog,
    ProjectSettings,
    TableExport,
    KeybindingsDialog,
}

const CONFIG: &str = include_str!("../.config/config.json5");

#[derive(Clone, Debug, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub data_dir: PathBuf,
    #[serde(default)]
    pub config_dir: PathBuf,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default, flatten)]
    pub config: AppConfig,
    #[serde(default)]
    pub keybindings: KeyBindings,
    #[serde(default)]
    pub styles: Styles,
}

lazy_static! {
    pub static ref PROJECT_NAME: String = env!("CARGO_CRATE_NAME").to_uppercase().to_string();
    pub static ref DATA_FOLDER: Option<PathBuf> =
        env::var(format!("{}_DATA", PROJECT_NAME.clone()))
            .ok()
            .map(PathBuf::from);
    pub static ref CONFIG_FOLDER: Option<PathBuf> =
        env::var(format!("{}_CONFIG", PROJECT_NAME.clone()))
            .ok()
            .map(PathBuf::from);
}

impl Config {
    pub fn from_path(config_path: Option<&PathBuf>) -> Result<Self, config::ConfigError> {
        let default_config: Config = json5::from_str(CONFIG).unwrap();
        let data_dir = get_data_dir();
        let config_dir = get_config_dir();
        let mut builder = config::Config::builder()
            .set_default("data_dir", data_dir.to_str().unwrap())?
            .set_default("config_dir", config_dir.to_str().unwrap())?;

        // Determine primary config file path
        let home_cfg = default_home_config_path();
        let selected_path = if let Some(p) = config_path {
            expand_tilde(p)
        } else {
            // Ensure default file exists at ~/.datatui-config.json5
            if !home_cfg.exists() {
                // Write embedded defaults
                if let Some(parent) = home_cfg.parent() { let _ = fs::create_dir_all(parent); }
                let _ = fs::write(&home_cfg, CONFIG);
            }
            home_cfg
        };

        builder = builder.add_source(
            config::File::from(selected_path).format(config::FileFormat::Json5).required(true),
        );

        let mut cfg: Self = builder.build()?.try_deserialize()?;

        for (mode, default_bindings) in default_config.keybindings.0.iter() {
            let user_bindings = cfg.keybindings.0.entry(*mode).or_default();
            for (key, cmd) in default_bindings.iter() {
                user_bindings
                    .entry(key.clone())
                    .or_insert_with(|| cmd.clone());
            }
        }
        for (mode, default_styles) in default_config.styles.0.iter() {
            let user_styles = cfg.styles.0.entry(*mode).or_default();
            for (style_key, style) in default_styles.iter() {
                user_styles.entry(style_key.clone()).or_insert(*style);
            }
        }

        Ok(cfg)
    }

    /// Build instructions string from list of (mode, action) tuples
    pub fn actions_to_instructions(&self, actions: &[(Mode, Action)]) -> String {
        actions.iter()
            .map(|(mode, action)| {
                let friendly_name = self.action_to_friendly_name(action);
                if let Some(key) = self.key_for_action(*mode, action) {
                    format!("{key}: {friendly_name}")
                } else {
                    friendly_name.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("  ")
    }

    /// Convert an action to a friendly name
    pub fn action_to_friendly_name(&self,action: &Action) -> &'static str {
        match action {
            // Global actions
            Action::Escape => "Esc",
            Action::Enter => "Enter",
            Action::Backspace => "Backspace",
            Action::Up => "Up",
            Action::Down => "Down",
            Action::Left => "Left",
            Action::Right => "Right",
            Action::Tab => "Tab",
            Action::Paste => "Paste",
            Action::ToggleInstructions => "Toggle Instructions",
            
            // DataTableContainer actions
            Action::OpenSortDialog => "Sort",
            Action::QuickSortCurrentColumn => "Quick Sort",
            Action::OpenFilterDialog => "Filter",
            Action::QuickFilterEqualsCurrentValue => "Quick Filter",
            Action::MoveSelectedColumnLeft => "Move Column Left",
            Action::MoveSelectedColumnRight => "Move Column Right",
            Action::OpenSqlDialog => "SQL",
            Action::OpenJmesDialog => "JMESPath",
            Action::OpenColumnOperationsDialog => "Column Ops",
            Action::OpenFindDialog => "Find",
            Action::OpenDataframeDetailsDialog => "Details",
            Action::OpenColumnWidthDialog => "Column Width",
            Action::OpenDataExportDialog => "Export",
            Action::CopySelectedCell => "Copy",
            
            // DataTabManager actions
            Action::OpenProjectSettingsDialog => "Settings",
            Action::OpenDataManagementDialog => "Data Management",
            Action::MoveTabToFront => "Move Tab Front",
            Action::MoveTabToBack => "Move Tab Back",
            Action::MoveTabLeft => "Move Tab Left",
            Action::MoveTabRight => "Move Tab Right",
            Action::PrevTab => "Prev Tab",
            Action::NextTab => "Next Tab",
            Action::SyncTabs => "Sync Tabs",
            
            // Dialog actions
            Action::DeleteSelectedSource => "Delete Source",
            Action::LoadAllPendingDatasets => "Load All",
            Action::EditSelectedAlias => "Edit Alias",
            Action::OpenDataImportDialog => "Import",
            Action::ConfirmDataImport => "Confirm Import",
            Action::DataImportSelect => "Select",
            Action::DataImportBack => "Back",
            Action::OpenFileBrowser => "Browse Files",
            
            // Sort dialog actions
            Action::ToggleSortDirection => "Toggle Sort",
            Action::RemoveSortColumn => "Remove Sort",
            Action::AddSortColumn => "Add Sort",
            
            // Filter dialog actions
            Action::AddFilter => "Add Filter",
            Action::EditFilter => "Edit Filter",
            Action::DeleteFilter => "Delete Filter",
            Action::AddFilterGroup => "Add Group",
            Action::SaveFilter => "Save Filter",
            Action::LoadFilter => "Load Filter",
            Action::ResetFilters => "Reset Filters",
            Action::ToggleFilterGroupType => "Toggle Group",
            
            // Find dialog actions
            Action::ToggleSpace => "Toggle Space",
            Action::Delete => "Delete",
            Action::GoToFirst => "First",
            Action::GoToLast => "Last",
            Action::PageUp => "Page Up",
            Action::PageDown => "Page Down",
            
            // SQL dialog actions
            Action::SelectAllText => "Select All",
            Action::CopyText => "Copy Text",
            Action::RunQuery => "Run Query",
            Action::CreateNewDataset => "New Dataset",
            Action::RestoreDataFrame => "Restore",
            Action::OpenSqlFileBrowser => "Browse SQL",
            Action::ClearText => "Clear",
            Action::PasteText => "Paste Text",
            
            // File browser actions
            Action::FileBrowserPageUp => "Page Up",
            Action::FileBrowserPageDown => "Page Down",
            Action::ConfirmOverwrite => "Confirm",
            Action::DenyOverwrite => "Deny",
            Action::NavigateToParent => "Parent Dir",
            
            // Column width dialog actions
            Action::ToggleAutoExpand => "Auto Expand",
            Action::ToggleColumnHidden => "Hide Column",
            Action::MoveColumnUp => "Move Up",
            Action::MoveColumnDown => "Move Down",
            
            // JMESPath dialog actions
            Action::AddColumn => "Add Column",
            Action::EditColumn => "Edit Column",
            Action::DeleteColumn => "Delete Column",
            Action::ApplyTransform => "Apply",
            
            // ColumnOperationOptions dialog actions
            Action::ToggleField => "Toggle Field",
            Action::ToggleButtons => "Toggle Buttons",
            
            // DataFrameDetails dialog actions
            Action::SwitchToNextTab => "Next Tab",
            Action::SwitchToPrevTab => "Prev Tab",
            Action::ChangeColumnLeft => "Change Column Left",
            Action::ChangeColumnRight => "Change Column Right",
            Action::OpenSortChoice => "Open Sort Choice",
            Action::OpenCastOverlay => "Open Cast Overlay",
            Action::AddFilterFromValue => "Add Filter From Value",
            Action::ExportCurrentTab => "Export Current Tab",
            Action::NavigateHeatmapLeft => "Heatmap Left",
            Action::NavigateHeatmapRight => "Heatmap Right",
            Action::NavigateHeatmapUp => "Heatmap Up",
            Action::NavigateHeatmapDown => "Heatmap Down",
            Action::NavigateHeatmapPageUp => "Heatmap Page Up",
            Action::NavigateHeatmapPageDown => "Heatmap Page Down",
            Action::NavigateHeatmapHome => "Heatmap Home",
            Action::NavigateHeatmapEnd => "Heatmap End",
            Action::ScrollStatsLeft => "Scroll Stats Left",
            Action::ScrollStatsRight => "Scroll Stats Right",
            
            // ProjectSettings dialog actions
            Action::ToggleDataViewerOption => "Toggle Option",
            // DataExport dialog actions
            Action::ToggleFormat => "Toggle Format",
            
            // TableExport dialog actions
            Action::CopyFilePath => "Copy Path",
            Action::ExportTable => "Export",
            // KeybindingsDialog actions
            Action::OpenGroupingDropdown => "Open Grouping",
            Action::SelectNextGrouping => "Next Group",
            Action::SelectPrevGrouping => "Prev Group",
            Action::StartRebinding => "Start Rebind",
            Action::ConfirmRebinding => "Apply Rebind",
            Action::CancelRebinding => "Cancel Rebind",
            Action::ClearBinding => "Clear Binding",
            Action::SaveKeybindings => "Save Keybindings",
            Action::ResetKeybindings => "Reset Keybindings",
            Action::SaveKeybindingsAs => "Save As",
            
            // Other actions
            Action::Quit => "Quit",
            Action::Suspend => "Suspend",
            Action::Help => "Help",
            Action::DialogClose => "Close",
            
            // Default to the debug representation for unknown actions
            _ => "Unknown",
        }
    }
    
    /// Resolve an action for a full key sequence for a given mode.
    pub fn action_for_keys(&self, mode: Mode, keys: &[KeyEvent]) -> Option<Action> {
        let map = self.keybindings.0.get(&mode)?;
        map.get(&keys.to_vec()).cloned()
    }

    /// Resolve an action for a single key event for a given mode.
    pub fn action_for_key(&self, mode: Mode, key: KeyEvent) -> Option<Action> {
        if key.kind != crossterm::event::KeyEventKind::Press {
            return None;
        }
        self.action_for_keys(mode, &[key])
    }

    /// Find the key for a given action in a specific mode
    pub fn key_for_action(&self, mode: Mode, action: &Action) -> Option<String> {
        let mode_bindings = self.keybindings.0.get(&mode)?;
        for (key_sequence, bound_action) in mode_bindings.iter() {
            if bound_action == action {
                return Some(key_sequence.iter()
                    .map(key_event_to_string)
                    .collect::<Vec<_>>()
                    .join(" "));
            }
        }
        None
    }

    /// Reset keybindings back to application defaults
    pub fn reset_keybindings_to_default(&mut self) {
        if let Ok(default_cfg) = json5::from_str::<Config>(CONFIG) {
            self.keybindings = default_cfg.keybindings;
        }
    }

    /// Serialize only the keybindings portion to JSON5 compatible with the app config format
    pub fn keybindings_to_json5(&self) -> String {
        let mut out: HashMap<Mode, HashMap<String, Action>> = HashMap::new();
        for (mode, inner) in self.keybindings.0.iter() {
            let mut m: HashMap<String, Action> = HashMap::new();
            for (seq, action) in inner.iter() {
                let parts: Vec<String> = seq.iter().map(key_event_to_string).collect();
                let key = format!("<{}>", parts.join("><"));
                m.insert(key, action.clone());
            }
            out.insert(*mode, m);
        }
        #[derive(Serialize)]
        struct KeybindingsExport<'a> {
            keybindings: &'a HashMap<Mode, HashMap<String, Action>>,
        }
        let wrapper = KeybindingsExport { keybindings: &out };
        json5::to_string(&wrapper).unwrap_or_else(|_| "{ keybindings: {} }".to_string())
    }
}

fn expand_tilde(path: &PathBuf) -> PathBuf {
    if let Some(s) = path.to_str() {
        if s.starts_with("~") {
            if let Some(base) = BaseDirs::new() { return PathBuf::from(s.replacen("~", base.home_dir().to_str().unwrap_or(""), 1)); }
        }
    }
    path.clone()
}

fn default_home_config_path() -> PathBuf {
    if let Some(base) = BaseDirs::new() {
        return base.home_dir().join(".datatui-config.json5");
    }
    PathBuf::from(".datatui-config.json5")
}

pub fn get_data_dir() -> PathBuf {
    if let Some(s) = DATA_FOLDER.clone() {
        s
    } else {
        PathBuf::from(".").join(".data")
    }
}

pub fn get_config_dir() -> PathBuf {
    if let Some(s) = CONFIG_FOLDER.clone() {
        s
    } else {
        PathBuf::from(".").join(".config")
    }
}

#[derive(Clone, Debug, Default, Deref, DerefMut)]
pub struct KeyBindings(pub HashMap<Mode, HashMap<Vec<KeyEvent>, Action>>);

impl<'de> Deserialize<'de> for KeyBindings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let parsed_map = HashMap::<Mode, HashMap<String, Action>>::deserialize(deserializer)?;

        let keybindings: HashMap<Mode, HashMap<Vec<KeyEvent>, Action>> = parsed_map
            .into_iter()
            .map(|(mode, inner_map)| {
                let converted_inner_map: HashMap<Vec<KeyEvent>, Action> = inner_map
                    .into_iter()
                    .map(|(key_string, action)| (parse_key_sequence(&key_string).unwrap(), action))
                    .collect();
                (mode, converted_inner_map)
            })
            .collect();

        Ok(KeyBindings(keybindings))
    }
}

fn parse_key_event(raw: &str) -> Result<KeyEvent, String> {
    let raw_lower = raw.to_ascii_lowercase();
    let (remaining, modifiers) = extract_modifiers(&raw_lower);
    parse_key_code_with_modifiers(remaining, modifiers)
}

fn extract_modifiers(raw: &str) -> (&str, KeyModifiers) {
    let mut modifiers = KeyModifiers::empty();
    let mut current = raw;

    loop {
        match current {
            rest if rest.starts_with("ctrl-") => {
                modifiers.insert(KeyModifiers::CONTROL);
                current = &rest[5..];
            }
            rest if rest.starts_with("alt-") => {
                modifiers.insert(KeyModifiers::ALT);
                current = &rest[4..];
            }
            rest if rest.starts_with("shift-") => {
                modifiers.insert(KeyModifiers::SHIFT);
                current = &rest[6..];
            }
            _ => break, // break out of the loop if no known prefix is detected
        };
    }

    (current, modifiers)
}

fn parse_key_code_with_modifiers(
    raw: &str,
    mut modifiers: KeyModifiers,
) -> Result<KeyEvent, String> {
    let c = match raw {
        "esc" => KeyCode::Esc,
        "enter" => KeyCode::Enter,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "backtab" => {
            modifiers.insert(KeyModifiers::SHIFT);
            KeyCode::BackTab
        }
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "f1" => KeyCode::F(1),
        "f2" => KeyCode::F(2),
        "f3" => KeyCode::F(3),
        "f4" => KeyCode::F(4),
        "f5" => KeyCode::F(5),
        "f6" => KeyCode::F(6),
        "f7" => KeyCode::F(7),
        "f8" => KeyCode::F(8),
        "f9" => KeyCode::F(9),
        "f10" => KeyCode::F(10),
        "f11" => KeyCode::F(11),
        "f12" => KeyCode::F(12),
        "space" => KeyCode::Char(' '),
        "hyphen" => KeyCode::Char('-'),
        "minus" => KeyCode::Char('-'),
        "tab" => KeyCode::Tab,
        c if c.len() == 1 => {
            let mut c = c.chars().next().unwrap();
            if modifiers.contains(KeyModifiers::SHIFT) {
                c = c.to_ascii_uppercase();
            }
            KeyCode::Char(c)
        }
        _ => return Err(format!("Unable to parse {raw}")),
    };
    Ok(KeyEvent::new(c, modifiers))
}

pub fn key_event_to_string(key_event: &KeyEvent) -> String {
    let char;
    let key_code = match key_event.code {
        KeyCode::Backspace => "backspace",
        KeyCode::Enter => "enter",
        KeyCode::Left => "left",
        KeyCode::Right => "right",
        KeyCode::Up => "up",
        KeyCode::Down => "down",
        KeyCode::Home => "home",
        KeyCode::End => "end",
        KeyCode::PageUp => "pageup",
        KeyCode::PageDown => "pagedown",
        KeyCode::Tab => "tab",
        KeyCode::BackTab => "backtab",
        KeyCode::Delete => "delete",
        KeyCode::Insert => "insert",
        KeyCode::F(c) => {
            char = format!("f({c})");
            &char
        }
        KeyCode::Char(' ') => "space",
        KeyCode::Char(c) => {
            char = c.to_string();
            &char
        }
        KeyCode::Esc => "esc",
        KeyCode::Null => "",
        KeyCode::CapsLock => "",
        KeyCode::Menu => "",
        KeyCode::ScrollLock => "",
        KeyCode::Media(_) => "",
        KeyCode::NumLock => "",
        KeyCode::PrintScreen => "",
        KeyCode::Pause => "",
        KeyCode::KeypadBegin => "",
        KeyCode::Modifier(_) => "",
    };

    let mut modifiers = Vec::with_capacity(3);

    if key_event.modifiers.intersects(KeyModifiers::CONTROL) {
        modifiers.push("ctrl");
    }

    if key_event.modifiers.intersects(KeyModifiers::SHIFT) {
        modifiers.push("shift");
    }

    if key_event.modifiers.intersects(KeyModifiers::ALT) {
        modifiers.push("alt");
    }

    let mut key = modifiers.join("-");

    if !key.is_empty() {
        key.push('-');
    }
    key.push_str(key_code);

    key
}

pub fn parse_key_sequence(raw: &str) -> Result<Vec<KeyEvent>, String> {
    if raw.chars().filter(|c| *c == '>').count() != raw.chars().filter(|c| *c == '<').count() {
        return Err(format!("Unable to parse `{raw}`"));
    }
    let raw = if !raw.contains("><") {
        let raw = raw.strip_prefix('<').unwrap_or(raw);
        
        raw.strip_prefix('>').unwrap_or(raw)
    } else {
        raw
    };
    let sequences = raw
        .split("><")
        .map(|seq| {
            if let Some(s) = seq.strip_prefix('<') {
                s
            } else if let Some(s) = seq.strip_suffix('>') {
                s
            } else {
                seq
            }
        })
        .collect::<Vec<_>>();

    sequences.into_iter().map(parse_key_event).collect()
}

#[derive(Clone, Debug, Default, Deref, DerefMut)]
pub struct Styles(pub HashMap<Mode, HashMap<String, Style>>);

impl<'de> Deserialize<'de> for Styles {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let parsed_map = HashMap::<Mode, HashMap<String, String>>::deserialize(deserializer)?;

        let styles: HashMap<Mode, HashMap<String, Style>> = parsed_map
            .into_iter()
            .map(|(mode, inner_map)| {
                let converted_inner_map: HashMap<String, Style> = inner_map
                    .into_iter()
                    .map(|(key, style_string)| (key, parse_style(&style_string)))
                    .collect();
                (mode, converted_inner_map)
            })
            .collect();

        Ok(Styles(styles))
    }
}

pub fn parse_style(line: &str) -> Style {
    let (foreground, background) =
        line.split_at(line.to_lowercase().find("on ").unwrap_or(line.len()));
    let foreground = process_color_string(foreground);
    let background = process_color_string(&background.replace("on ", ""));

    let mut style = Style::default();
    if let Some(fg) = parse_color(&foreground.0) {
        style = style.fg(fg);
    }
    if let Some(bg) = parse_color(&background.0) {
        style = style.bg(bg);
    }
    style = style.add_modifier(foreground.1 | background.1);
    style
}

fn process_color_string(color_str: &str) -> (String, Modifier) {
    let color = color_str
        .replace("grey", "gray")
        .replace("bright ", "")
        .replace("bold ", "")
        .replace("underline ", "")
        .replace("inverse ", "");

    let mut modifiers = Modifier::empty();
    if color_str.contains("underline") {
        modifiers |= Modifier::UNDERLINED;
    }
    if color_str.contains("bold") {
        modifiers |= Modifier::BOLD;
    }
    if color_str.contains("inverse") {
        modifiers |= Modifier::REVERSED;
    }

    (color, modifiers)
}

fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim_start();
    let s = s.trim_end();
    if s.contains("bright color") {
        let s = s.trim_start_matches("bright ");
        let c = s
            .trim_start_matches("color")
            .parse::<u8>()
            .unwrap_or_default();
        Some(Color::Indexed(c.wrapping_shl(8)))
    } else if s.contains("color") {
        let c = s
            .trim_start_matches("color")
            .parse::<u8>()
            .unwrap_or_default();
        Some(Color::Indexed(c))
    } else if s.contains("gray") {
        let c = 232
            + s.trim_start_matches("gray")
                .parse::<u8>()
                .unwrap_or_default();
        Some(Color::Indexed(c))
    } else if s.contains("rgb") {
        let red = (s.as_bytes()[3] as char).to_digit(10).unwrap_or_default() as u8;
        let green = (s.as_bytes()[4] as char).to_digit(10).unwrap_or_default() as u8;
        let blue = (s.as_bytes()[5] as char).to_digit(10).unwrap_or_default() as u8;
        let c = 16 + red * 36 + green * 6 + blue;
        Some(Color::Indexed(c))
    } else if s == "bold black" {
        Some(Color::Indexed(8))
    } else if s == "bold red" {
        Some(Color::Indexed(9))
    } else if s == "bold green" {
        Some(Color::Indexed(10))
    } else if s == "bold yellow" {
        Some(Color::Indexed(11))
    } else if s == "bold blue" {
        Some(Color::Indexed(12))
    } else if s == "bold magenta" {
        Some(Color::Indexed(13))
    } else if s == "bold cyan" {
        Some(Color::Indexed(14))
    } else if s == "bold white" {
        Some(Color::Indexed(15))
    } else if s == "black" {
        Some(Color::Indexed(0))
    } else if s == "red" {
        Some(Color::Indexed(1))
    } else if s == "green" {
        Some(Color::Indexed(2))
    } else if s == "yellow" {
        Some(Color::Indexed(3))
    } else if s == "blue" {
        Some(Color::Indexed(4))
    } else if s == "magenta" {
        Some(Color::Indexed(5))
    } else if s == "cyan" {
        Some(Color::Indexed(6))
    } else if s == "white" {
        Some(Color::Indexed(7))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_parse_style_default() {
        let style = parse_style("");
        assert_eq!(style, Style::default());
    }

    #[test]
    fn test_parse_style_foreground() {
        let style = parse_style("red");
        assert_eq!(style.fg, Some(Color::Indexed(1)));
    }

    #[test]
    fn test_parse_style_background() {
        let style = parse_style("on blue");
        assert_eq!(style.bg, Some(Color::Indexed(4)));
    }

    #[test]
    fn test_parse_style_modifiers() {
        let style = parse_style("underline red on blue");
        assert_eq!(style.fg, Some(Color::Indexed(1)));
        assert_eq!(style.bg, Some(Color::Indexed(4)));
    }

    #[test]
    fn test_process_color_string() {
        let (color, modifiers) = process_color_string("underline bold inverse gray");
        assert_eq!(color, "gray");
        assert!(modifiers.contains(Modifier::UNDERLINED));
        assert!(modifiers.contains(Modifier::BOLD));
        assert!(modifiers.contains(Modifier::REVERSED));
    }

    #[test]
    fn test_parse_color_rgb() {
        let color = parse_color("rgb123");
        let expected = 16 + 36 + 2 * 6 + 3;
        assert_eq!(color, Some(Color::Indexed(expected)));
    }

    #[test]
    fn test_parse_color_unknown() {
        let color = parse_color("unknown");
        assert_eq!(color, None);
    }

    #[test]
    fn test_simple_keys() {
        assert_eq!(
            parse_key_event("a").unwrap(),
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty())
        );

        assert_eq!(
            parse_key_event("enter").unwrap(),
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty())
        );

        assert_eq!(
            parse_key_event("esc").unwrap(),
            KeyEvent::new(KeyCode::Esc, KeyModifiers::empty())
        );
    }

    #[test]
    fn test_with_modifiers() {
        assert_eq!(
            parse_key_event("ctrl-a").unwrap(),
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)
        );

        assert_eq!(
            parse_key_event("alt-enter").unwrap(),
            KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT)
        );

        assert_eq!(
            parse_key_event("shift-esc").unwrap(),
            KeyEvent::new(KeyCode::Esc, KeyModifiers::SHIFT)
        );
    }

    #[test]
    fn test_multiple_modifiers() {
        assert_eq!(
            parse_key_event("ctrl-alt-a").unwrap(),
            KeyEvent::new(
                KeyCode::Char('a'),
                KeyModifiers::CONTROL | KeyModifiers::ALT
            )
        );

        assert_eq!(
            parse_key_event("ctrl-shift-enter").unwrap(),
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL | KeyModifiers::SHIFT)
        );
    }

    #[test]
    fn test_reverse_multiple_modifiers() {
        assert_eq!(
            key_event_to_string(&KeyEvent::new(
                KeyCode::Char('a'),
                KeyModifiers::CONTROL | KeyModifiers::ALT
            )),
            "ctrl-alt-a".to_string()
        );
    }

    #[test]
    fn test_invalid_keys() {
        assert!(parse_key_event("invalid-key").is_err());
        assert!(parse_key_event("ctrl-invalid-key").is_err());
    }

    #[test]
    fn test_case_insensitivity() {
        assert_eq!(
            parse_key_event("CTRL-a").unwrap(),
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)
        );

        assert_eq!(
            parse_key_event("AlT-eNtEr").unwrap(),
            KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT)
        );
    }
}
