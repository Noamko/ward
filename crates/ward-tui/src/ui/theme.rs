use ratatui::style::{Color, Modifier, Style};

pub const BG: Color = Color::Rgb(18, 18, 24);
pub const SURFACE: Color = Color::Rgb(28, 28, 38);
pub const BORDER: Color = Color::Rgb(60, 60, 80);
pub const BORDER_ACTIVE: Color = Color::Rgb(120, 100, 220);
pub const TEXT: Color = Color::Rgb(220, 220, 240);
pub const TEXT_DIM: Color = Color::Rgb(110, 110, 140);
pub const ACCENT: Color = Color::Rgb(120, 100, 220);
pub const GREEN: Color = Color::Rgb(80, 200, 120);
pub const RED: Color = Color::Rgb(220, 80, 80);
pub const YELLOW: Color = Color::Rgb(220, 180, 60);
#[allow(dead_code)]
pub const BLUE: Color = Color::Rgb(80, 140, 220);
pub const DONE_FG: Color = Color::Rgb(80, 80, 100);

pub fn base() -> Style {
    Style::default().fg(TEXT).bg(BG)
}

pub fn dim() -> Style {
    Style::default().fg(TEXT_DIM)
}

pub fn active_border() -> Style {
    Style::default().fg(BORDER_ACTIVE)
}

pub fn inactive_border() -> Style {
    Style::default().fg(BORDER)
}

pub fn selected() -> Style {
    Style::default()
        .bg(Color::Rgb(40, 36, 60))
        .fg(TEXT)
        .add_modifier(Modifier::BOLD)
}

pub fn done() -> Style {
    Style::default()
        .fg(DONE_FG)
        .add_modifier(Modifier::CROSSED_OUT)
}

pub fn overdue() -> Style {
    Style::default().fg(RED).add_modifier(Modifier::BOLD)
}

pub fn due_today() -> Style {
    Style::default().fg(YELLOW)
}

pub fn priority_high() -> Style {
    Style::default().fg(RED).add_modifier(Modifier::BOLD)
}

pub fn priority_medium() -> Style {
    Style::default().fg(YELLOW)
}

pub fn priority_low() -> Style {
    Style::default().fg(TEXT_DIM)
}

pub fn accent() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

pub fn key_hint() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

pub fn key_desc() -> Style {
    Style::default().fg(TEXT_DIM)
}
