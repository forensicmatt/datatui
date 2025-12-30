use crate::tui::action::Action;
use color_eyre::Result;
use ratatui::{layout::Rect, Frame};

/// Base trait for all TUI components
///
/// All interactive UI elements implement this trait to provide consistent
/// behavior for action handling, rendering, and component lifecycle.
pub trait Component {
    /// Handle an action
    ///
    /// Returns Ok(true) if the action was handled and consumed.
    /// Returns Ok(false) if the action was not handled and should propagate.
    /// Returns Err if handling the action resulted in an error.
    fn handle_action(&mut self, action: Action) -> Result<bool>;

    /// Render the component to the terminal
    ///
    /// Components are responsible for rendering themselves within the given area.
    fn render(&mut self, frame: &mut Frame, area: Rect);

    /// Get list of actions this component supports
    ///
    /// Used for generating context-sensitive help and validating action routing.
    fn supported_actions(&self) -> &[Action];

    /// Get component name for debugging/logging
    fn name(&self) -> &str;

    /// Update component state (called on every tick)
    ///
    /// Default implementation does nothing. Override if component needs
    /// to update state independently of user input.
    fn update(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Focusable component trait
///
/// Components that can receive keyboard focus implement this trait.
/// Focus determines which component receives keyboard input.
pub trait Focusable: Component {
    /// Check if component currently has focus
    fn is_focused(&self) -> bool;

    /// Set focus state
    fn set_focused(&mut self, focused: bool);
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock component for testing
    struct MockComponent {
        name: String,
        focused: bool,
        actions: Vec<Action>,
    }

    impl MockComponent {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                focused: false,
                actions: vec![Action::MoveUp, Action::MoveDown],
            }
        }
    }

    impl Component for MockComponent {
        fn handle_action(&mut self, action: Action) -> Result<bool> {
            if self.supported_actions().contains(&action) {
                Ok(true) // Handled
            } else {
                Ok(false) // Not handled
            }
        }

        fn render(&mut self, _frame: &mut Frame, _area: Rect) {
            // Mock render - does nothing
        }

        fn supported_actions(&self) -> &[Action] {
            &self.actions
        }

        fn name(&self) -> &str {
            &self.name
        }
    }

    impl Focusable for MockComponent {
        fn is_focused(&self) -> bool {
            self.focused
        }

        fn set_focused(&mut self, focused: bool) {
            self.focused = focused;
        }
    }

    #[test]
    fn test_component_action_handling() {
        let mut comp = MockComponent::new("test");

        // Supported action should be handled
        assert!(comp.handle_action(Action::MoveUp).unwrap());

        // Unsupported action should not be handled
        assert!(!comp.handle_action(Action::Quit).unwrap());
    }

    #[test]
    fn test_focusable() {
        let mut comp = MockComponent::new("test");

        assert!(!comp.is_focused());
        comp.set_focused(true);
        assert!(comp.is_focused());
        comp.set_focused(false);
        assert!(!comp.is_focused());
    }

    #[test]
    fn test_component_metadata() {
        let comp = MockComponent::new("test_comp");

        assert_eq!(comp.name(), "test_comp");
        assert_eq!(comp.supported_actions().len(), 2);
    }
}
