//! StyleSetManager: Manages loading, saving, and enabling/disabling StyleSets
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::fs;
use color_eyre::Result;
use serde_yaml;
use crate::dialog::styling::style_set::StyleSet;

/// Manages all StyleSets, including loading from folders and tracking enabled sets
#[derive(Debug, Clone)]
pub struct StyleSetManager {
    /// All loaded style sets, keyed by their identifier (name or path)
    style_sets: BTreeMap<String, StyleSet>,
    /// Set of enabled style set identifiers
    enabled_sets: std::collections::HashSet<String>,
    /// Folders that have been loaded
    loaded_folders: Vec<PathBuf>,
}

impl StyleSetManager {
    /// Create a new StyleSetManager
    pub fn new() -> Self {
        Self {
            style_sets: BTreeMap::new(),
            enabled_sets: std::collections::HashSet::new(),
            loaded_folders: Vec::new(),
        }
    }

    /// Load all YAML style set files from a folder
    pub fn load_from_folder(&mut self, folder_path: &Path) -> Result<Vec<String>> {
        let mut loaded_names = Vec::new();
        
        if !folder_path.is_dir() {
            return Err(color_eyre::eyre::eyre!("Path is not a directory: {}", folder_path.display()));
        }

        // Read all .yaml and .yml files in the folder
        for entry in fs::read_dir(folder_path)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_file() {
                let ext = path.extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                
                if ext == "yaml" || ext == "yml" {
                    match self.load_from_file(&path) {
                        Ok(name) => {
                            loaded_names.push(name);
                        }
                        Err(e) => {
                            tracing::warn!("Failed to load style set from {}: {}", path.display(), e);
                        }
                    }
                }
            }
        }

        // Track this folder as loaded
        if !self.loaded_folders.contains(&folder_path.to_path_buf()) {
            self.loaded_folders.push(folder_path.to_path_buf());
        }

        Ok(loaded_names)
    }

    /// Load a single style set from a YAML file
    pub fn load_from_file(&mut self, file_path: &Path) -> Result<String> {
        let content = fs::read_to_string(file_path)?;
        let style_set: StyleSet = serde_yaml::from_str(&content)
            .map_err(|e| color_eyre::eyre::eyre!("Failed to parse YAML: {}", e))?;
        
        // Use name as identifier, or file name if name is empty
        let identifier = if style_set.name.is_empty() {
            file_path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unnamed")
                .to_string()
        } else {
            style_set.name.clone()
        };

        self.style_sets.insert(identifier.clone(), style_set);
        Ok(identifier)
    }

    /// Save a style set to a YAML file
    pub fn save_to_file(&self, style_set: &StyleSet, file_path: &Path) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let yaml = serde_yaml::to_string(style_set)
            .map_err(|e| color_eyre::eyre::eyre!("Failed to serialize to YAML: {}", e))?;
        
        fs::write(file_path, yaml)?;
        Ok(())
    }

    /// Enable a style set by identifier
    pub fn enable_style_set(&mut self, identifier: &str) -> bool {
        if self.style_sets.contains_key(identifier) {
            self.enabled_sets.insert(identifier.to_string());
            true
        } else {
            false
        }
    }

    /// Disable a style set by identifier
    pub fn disable_style_set(&mut self, identifier: &str) {
        self.enabled_sets.remove(identifier);
    }

    /// Check if a style set is enabled
    pub fn is_enabled(&self, identifier: &str) -> bool {
        self.enabled_sets.contains(identifier)
    }

    /// Get all enabled style sets
    pub fn get_enabled_sets(&self) -> Vec<&StyleSet> {
        self.enabled_sets.iter()
            .filter_map(|id| self.style_sets.get(id))
            .collect()
    }

    /// Get all style sets (enabled and disabled)
    pub fn get_all_sets(&self) -> Vec<(&String, &StyleSet, bool)> {
        self.style_sets.iter()
            .map(|(id, set)| (id, set, self.enabled_sets.contains(id)))
            .collect()
    }

    /// Get a style set by identifier
    pub fn get_set(&self, identifier: &str) -> Option<&StyleSet> {
        self.style_sets.get(identifier)
    }

    /// Add a new style set
    pub fn add_set(&mut self, style_set: StyleSet) -> String {
        let identifier = if style_set.name.is_empty() {
            format!("style_set_{}", self.style_sets.len())
        } else {
            style_set.name.clone()
        };
        self.style_sets.insert(identifier.clone(), style_set);
        identifier
    }

    /// Remove a style set
    pub fn remove_set(&mut self, identifier: &str) -> bool {
        self.enabled_sets.remove(identifier);
        self.style_sets.remove(identifier).is_some()
    }

    /// Get all loaded folder paths
    pub fn get_loaded_folders(&self) -> &[PathBuf] {
        &self.loaded_folders
    }

    /// Clear all style sets
    pub fn clear(&mut self) {
        self.style_sets.clear();
        self.enabled_sets.clear();
        self.loaded_folders.clear();
    }

    /// Get enabled set identifiers (for serialization)
    pub fn get_enabled_identifiers(&self) -> Vec<String> {
        self.enabled_sets.iter().cloned().collect()
    }

    /// Set enabled set identifiers (for deserialization)
    pub fn set_enabled_identifiers(&mut self, identifiers: Vec<String>) {
        self.enabled_sets.clear();
        for id in identifiers {
            if self.style_sets.contains_key(&id) {
                self.enabled_sets.insert(id);
            }
        }
    }
}

impl Default for StyleSetManager {
    fn default() -> Self {
        Self::new()
    }
}


