use eframe::egui::InputState;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    ScrollLines(i32),
    HalfPage(i32),
    Page(i32),
    Top,
    Bottom,
    ToggleTheme,
    ToggleFullWidth,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Quit,
}

pub fn actions(_input: &InputState) -> Vec<Action> {
    todo!("K: key → Action")
}
