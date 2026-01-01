use crate::tui::action::Action;
use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Maps KeyEvents to Actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBindings {
    #[serde(rename = "bindings")]
    bindings_list: Vec<KeyBinding>,

    #[serde(skip)]
    bindings_map: HashMap<KeyPattern, Action>,
}

/// Single keybinding entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBinding {
    pub key: String,
    pub action: Action,
}

/// Pattern for matching key events
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyPattern {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyBindings {
    /// Create default keybindings
    pub fn default() -> Self {
        let bindings_list = vec![
            // Navigation - Arrow keys
            KeyBinding::new("Up", Action::MoveUp),
            KeyBinding::new("Down", Action::MoveDown),
            KeyBinding::new("Left", Action::MoveLeft),
            KeyBinding::new("Right", Action::MoveRight),
            // Navigation - Vim-style
            KeyBinding::new("k", Action::MoveUp),
            KeyBinding::new("j", Action::MoveDown),
            KeyBinding::new("h", Action::MoveLeft),
            KeyBinding::new("l", Action::MoveRight),
            // Page navigation
            KeyBinding::new("PageUp", Action::PageUp),
            KeyBinding::new("PageDown", Action::PageDown),
            KeyBinding::new("Ctrl+u", Action::PageUp),
            KeyBinding::new("Ctrl+d", Action::PageDown),
            // Home/End
            KeyBinding::new("Home", Action::Home),
            KeyBinding::new("End", Action::End),
            KeyBinding::new("0", Action::Home),
            KeyBinding::new("$", Action::End),
            // Top/Bottom
            KeyBinding::new("g", Action::GoToTop),
            KeyBinding::new("G", Action::GoToBottom),
            // Application
            KeyBinding::new("q", Action::Quit),
            KeyBinding::new("Esc", Action::Cancel),
            KeyBinding::new("Enter", Action::Confirm),
            // Help
            KeyBinding::new("?", Action::ToggleHelp),
            KeyBinding::new("F1", Action::ToggleHelp),
            // Data operations
            KeyBinding::new("s", Action::Sort),
            KeyBinding::new("f", Action::Filter),
            KeyBinding::new("Ctrl+f", Action::Find),
            KeyBinding::new("/", Action::Find),
            KeyBinding::new(":", Action::Query),
            // Refresh
            KeyBinding::new("r", Action::Refresh),
            KeyBinding::new("F5", Action::Refresh),
            // Tabs
            KeyBinding::new("Tab", Action::NextTab),
            KeyBinding::new("Shift+Tab", Action::PrevTab),
            KeyBinding::new("w", Action::CloseTab),
            KeyBinding::new("t", Action::NewTab),
            // Copy
            KeyBinding::new("c", Action::Copy),
            KeyBinding::new("C", Action::CopyWithHeaders),
            // Import/Export
            KeyBinding::new("o", Action::Import),
            KeyBinding::new("e", Action::Export),
        ];

        let bindings_map = Self::build_map(&bindings_list);

        Self {
            bindings_list,
            bindings_map,
        }
    }

    /// Build hashmap from bindings list
    fn build_map(bindings: &[KeyBinding]) -> HashMap<KeyPattern, Action> {
        bindings
            .iter()
            .filter_map(|b| {
                KeyPattern::from_string(&b.key)
                    .ok()
                    .map(|pattern| (pattern, b.action))
            })
            .collect()
    }

    /// Get action for key event
    pub fn get_action(&self, key: &KeyEvent) -> Option<Action> {
        let pattern = KeyPattern::from_event(key);
        self.bindings_map.get(&pattern).copied()
    }

    /// Load from JSON config file
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let mut bindings: KeyBindings = serde_json::from_str(&content)?;
        bindings.bindings_map = Self::build_map(&bindings.bindings_list);
        Ok(bindings)
    }

    /// Save to JSON config file
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Get all bindings for an action (for help display)
    pub fn get_keys_for_action(&self, action: Action) -> Vec<String> {
        self.bindings_list
            .iter()
            .filter(|b| b.action == action)
            .map(|b| b.key.clone())
            .collect()
    }

    /// Check for actions that don't have any keybindings
    /// Returns Vec of (Action, description) for unbound actions
    pub fn get_unbound_actions(&self) -> Vec<(Action, &'static str)> {
        let bound_actions: HashSet<Action> = self.bindings_list.iter().map(|b| b.action).collect();

        Action::all()
            .into_iter()
            .filter(|action| !bound_actions.contains(action))
            .map(|action| (action, action.description()))
            .collect()
    }

    /// Validate bindings and return warnings
    pub fn validate(&self) -> Vec<String> {
        let mut warnings = Vec::new();

        // Check for duplicate key bindings
        let mut seen_keys: HashMap<String, Action> = HashMap::new();
        for binding in &self.bindings_list {
            if let Some(existing_action) = seen_keys.get(&binding.key) {
                warnings.push(format!(
                    "Duplicate key '{}': bound to both {:?} and {:?}",
                    binding.key, existing_action, binding.action
                ));
            } else {
                seen_keys.insert(binding.key.clone(), binding.action);
            }
        }

        // Check for unbound actions
        let unbound = self.get_unbound_actions();
        if !unbound.is_empty() {
            warnings.push(format!(
                "Warning: {} action(s) have no keybindings: {}",
                unbound.len(),
                unbound
                    .iter()
                    .map(|(action, _)| format!("{:?}", action))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        // Check for invalid key patterns
        for binding in &self.bindings_list {
            if KeyPattern::from_string(&binding.key).is_err() {
                warnings.push(format!(
                    "Invalid key pattern '{}' for action {:?}",
                    binding.key, binding.action
                ));
            }
        }

        warnings
    }
}

impl KeyBinding {
    pub fn new(key: &str, action: Action) -> Self {
        Self {
            key: key.to_string(),
            action,
        }
    }
}

impl KeyPattern {
    pub fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    pub fn from_event(event: &KeyEvent) -> Self {
        Self {
            code: event.code,
            modifiers: event.modifiers,
        }
    }

    /// Parse from string (e.g., "Ctrl+C", "Shift+?", "a")
    pub fn from_string(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split('+').collect();

        let mut modifiers = KeyModifiers::empty();
        let key_part = if parts.len() > 1 {
            // Parse modifiers
            for part in &parts[..parts.len() - 1] {
                match part.to_lowercase().as_str() {
                    "ctrl" => modifiers |= KeyModifiers::CONTROL,
                    "alt" => modifiers |= KeyModifiers::ALT,
                    "shift" => modifiers |= KeyModifiers::SHIFT,
                    "cmd" | "command" | "super" => {
                        // Mac Command key maps to SUPER
                        #[cfg(target_os = "macos")]
                        {
                            modifiers |= KeyModifiers::SUPER;
                        }
                        #[cfg(not(target_os = "macos"))]
                        {
                            modifiers |= KeyModifiers::CONTROL; // Fallback to Ctrl on non-Mac
                        }
                    }
                    _ => return Err(format!("Unknown modifier: {}", part)),
                }
            }
            parts[parts.len() - 1]
        } else {
            // Handle special Shift cases (?, $, etc.)
            if s.len() == 1 {
                let ch = s.chars().next().unwrap();
                if ch.is_uppercase() || "!@#$%^&*()_+{}|:\"<>?".contains(ch) {
                    modifiers |= KeyModifiers::SHIFT;
                }
            }
            parts[0]
        };

        // Parse key code
        let code = match key_part.to_lowercase().as_str() {
            // Special keys first
            "up" | "↑" => KeyCode::Up,
            "down" | "↓" => KeyCode::Down,
            "left" | "←" => KeyCode::Left,
            "right" | "→" => KeyCode::Right,
            "pageup" | "pgup" => KeyCode::PageUp,
            "pagedown" | "pgdown" | "pgdn" => KeyCode::PageDown,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "tab" => KeyCode::Tab,
            "backtab" => KeyCode::BackTab,
            "enter" | "return" => KeyCode::Enter,
            "esc" | "escape" => KeyCode::Esc,
            "backspace" => KeyCode::Backspace,
            "delete" | "del" => KeyCode::Delete,
            "insert" | "ins" => KeyCode::Insert,
            "space" => KeyCode::Char(' '),

            // Single characters (must come before function key check to avoid matching 'f')
            s if s.len() == 1 => {
                let ch = s.chars().next().unwrap().to_ascii_lowercase();
                KeyCode::Char(ch)
            }

            // Function keys: F1-F12
            s if s.starts_with('f') && s.len() >= 2 && s.len() <= 3 => {
                if let Ok(n) = s[1..].parse::<u8>() {
                    if (1..=12).contains(&n) {
                        KeyCode::F(n)
                    } else {
                        return Err(format!("Invalid function key: {}", s));
                    }
                } else {
                    return Err(format!("Invalid function key: {}", s));
                }
            }

            _ => return Err(format!("Unknown key: {}", key_part)),
        };

        Ok(Self { code, modifiers })
    }

    /// Display as human-readable string
    pub fn to_string(&self) -> String {
        let mut parts = Vec::new();

        if self.modifiers.contains(KeyModifiers::SUPER) {
            #[cfg(target_os = "macos")]
            parts.push("Cmd");
            #[cfg(not(target_os = "macos"))]
            parts.push("Super");
        }
        if self.modifiers.contains(KeyModifiers::CONTROL) {
            parts.push("Ctrl");
        }
        if self.modifiers.contains(KeyModifiers::ALT) {
            parts.push("Alt");
        }
        if self.modifiers.contains(KeyModifiers::SHIFT) {
            parts.push("Shift");
        }

        let key_str = match self.code {
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Up => "↑".to_string(),
            KeyCode::Down => "↓".to_string(),
            KeyCode::Left => "←".to_string(),
            KeyCode::Right => "→".to_string(),
            KeyCode::PageUp => "PgUp".to_string(),
            KeyCode::PageDown => "PgDn".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::BackTab => "Shift+Tab".to_string(),
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Esc => "Esc".to_string(),
            KeyCode::Backspace => "Backspace".to_string(),
            KeyCode::Delete => "Del".to_string(),
            KeyCode::F(n) => format!("F{}", n),
            _ => format!("{:?}", self.code),
        };

        parts.push(&key_str);
        parts.join("+")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_pattern_parsing() {
        assert!(KeyPattern::from_string("Ctrl+c").is_ok());
        assert!(KeyPattern::from_string("a").is_ok());
        assert!(KeyPattern::from_string("F1").is_ok());
        assert!(KeyPattern::from_string("Up").is_ok());
        assert!(KeyPattern::from_string("Ctrl+Alt+Delete").is_ok());
    }

    #[test]
    fn test_mac_command_key() {
        let pattern = KeyPattern::from_string("Cmd+c").unwrap();
        #[cfg(target_os = "macos")]
        assert!(pattern.modifiers.contains(KeyModifiers::SUPER));
        #[cfg(not(target_os = "macos"))]
        assert!(pattern.modifiers.contains(KeyModifiers::CONTROL));
    }

    #[test]
    fn test_default_bindings_are_valid() {
        let bindings = KeyBindings::default();
        let warnings = bindings.validate();

        // Print warnings for debugging
        for warning in &warnings {
            eprintln!("Warning: {}", warning);
        }

        // Should have warning about unbound actions, but no invalid patterns
        for warning in &warnings {
            assert!(
                !warning.contains("Invalid key pattern"),
                "Found invalid pattern: {}",
                warning
            );
        }
    }

    #[test]
    fn test_unbound_actions() {
        let bindings = KeyBindings::default();
        let unbound = bindings.get_unbound_actions();

        // Should have some unbound actions
        assert!(!unbound.is_empty());

        // Each should have a description
        for (_, desc) in unbound {
            assert!(!desc.is_empty());
        }
    }

    #[test]
    fn test_save_and_load() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("keybindings.json");

        let bindings = KeyBindings::default();
        bindings.save_to_file(&path).unwrap();

        let loaded = KeyBindings::load_from_file(&path).unwrap();
        assert_eq!(bindings.bindings_list.len(), loaded.bindings_list.len());
    }
}
