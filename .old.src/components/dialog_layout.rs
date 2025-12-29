use ratatui::prelude::Rect;

/// Helper for dialog layout with optional instructions area
pub struct DialogLayout {
    pub content_area: Rect,
    pub instructions_area: Option<Rect>,
}
impl DialogLayout {
    pub fn new(content_area: Rect, instructions_area: Option<Rect>) -> Self {
        Self { content_area, instructions_area }
    }

    pub fn get_total_area(&self) -> Rect {
        let mut total_area = self.content_area;
        if let Some(instructions_area) = self.instructions_area {
            total_area.height += instructions_area.height;
        }
        total_area
    }
}

pub fn split_dialog_area(
    area: Rect,
    show_instructions: bool,
    instructions: Option<&str>,
) -> DialogLayout {
    if show_instructions {
        let wrap_width = area.width.saturating_sub(4).max(10) as usize;
        let instructions = instructions.unwrap_or("");
        let wrapped_lines = textwrap::wrap(instructions, wrap_width);
        let instructions_height = (wrapped_lines.len() as u16).max(1) + 2;
        let content_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: area.height.saturating_sub(instructions_height),
        };
        let instructions_area = Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(instructions_height),
            width: area.width,
            height: instructions_height,
        };
        DialogLayout {
            content_area,
            instructions_area: Some(instructions_area),
        }
    } else {
        DialogLayout {
            content_area: area,
            instructions_area: None,
        }
    }
} 