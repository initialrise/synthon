#![allow(dead_code)]

use eframe::egui::{Event, InputState, Key, Modifiers};

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

/// Actions for this frame's key presses (PRD §5.4). Key repeats fire too, except for toggles.
pub fn actions(input: &InputState) -> Vec<Action> {
    input
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Key {
                key,
                pressed: true,
                repeat,
                modifiers,
                ..
            } => map(*key, *modifiers).filter(|a| {
                !(*repeat && matches!(a, Action::ToggleTheme | Action::ToggleFullWidth))
            }),
            _ => None,
        })
        .collect()
}

fn map(key: Key, m: Modifiers) -> Option<Action> {
    use Action::*;
    if m.alt {
        return None;
    }
    if m.ctrl || m.command {
        // Shift is allowed so Ctrl+Shift+= (i.e. Ctrl++) also zooms.
        return Some(match key {
            Key::D => HalfPage(1),
            Key::U => HalfPage(-1),
            Key::Equals | Key::Plus => ZoomIn,
            Key::Minus => ZoomOut,
            Key::Num0 => ZoomReset,
            _ => return None,
        });
    }
    Some(match key {
        Key::ArrowDown => ScrollLines(1),
        Key::ArrowUp => ScrollLines(-1),
        Key::PageDown => Page(1),
        Key::PageUp => Page(-1),
        Key::Home => Top,
        Key::End => Bottom,
        Key::Escape => Quit,
        Key::G if m.shift => Bottom,
        _ if m.shift => return None,
        Key::J => ScrollLines(1),
        Key::K => ScrollLines(-1),
        Key::D => HalfPage(1),
        Key::U => HalfPage(-1),
        Key::G => Top,
        Key::T => ToggleTheme,
        Key::F => ToggleFullWidth,
        Key::Q => Quit,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::Action::*;
    use super::*;
    use eframe::egui::RawInput;

    const NONE: Modifiers = Modifiers::NONE;
    const CTRL: Modifiers = Modifiers {
        ctrl: true,
        command: true,
        ..Modifiers::NONE
    };
    const SHIFT: Modifiers = Modifiers::SHIFT;
    const CTRL_SHIFT: Modifiers = Modifiers {
        shift: true,
        ..CTRL
    };

    #[test]
    fn prd_table() {
        let rows = [
            (Key::J, NONE, ScrollLines(1)),
            (Key::ArrowDown, NONE, ScrollLines(1)),
            (Key::K, NONE, ScrollLines(-1)),
            (Key::ArrowUp, NONE, ScrollLines(-1)),
            (Key::D, NONE, HalfPage(1)),
            (Key::D, CTRL, HalfPage(1)),
            (Key::U, NONE, HalfPage(-1)),
            (Key::U, CTRL, HalfPage(-1)),
            (Key::PageDown, NONE, Page(1)),
            (Key::PageUp, NONE, Page(-1)),
            (Key::G, NONE, Top),
            (Key::Home, NONE, Top),
            (Key::G, SHIFT, Bottom),
            (Key::End, NONE, Bottom),
            (Key::T, NONE, ToggleTheme),
            (Key::F, NONE, ToggleFullWidth),
            (Key::Equals, CTRL, ZoomIn),
            (Key::Plus, CTRL, ZoomIn),
            (Key::Equals, CTRL_SHIFT, ZoomIn),
            (Key::Minus, CTRL, ZoomOut),
            (Key::Num0, CTRL, ZoomReset),
            (Key::Q, NONE, Quit),
            (Key::Escape, NONE, Quit),
        ];
        for (key, mods, want) in rows {
            assert_eq!(map(key, mods), Some(want), "{key:?} {mods:?}");
        }
    }

    #[test]
    fn modifiers_block_plain_letters() {
        let alt = Modifiers::ALT;
        for (key, mods) in [
            (Key::J, CTRL),
            (Key::T, CTRL),
            (Key::Q, alt),
            (Key::J, SHIFT),
            (Key::Equals, NONE),
            (Key::Num0, NONE),
        ] {
            assert_eq!(map(key, mods), None, "{key:?} {mods:?}");
        }
    }

    fn key(key: Key, pressed: bool, repeat: bool) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat,
            modifiers: NONE,
        }
    }

    #[test]
    fn actions_from_events() {
        let raw = RawInput {
            events: vec![
                key(Key::J, true, false),
                key(Key::J, false, false),
                key(Key::K, true, false),
                key(Key::X, true, false),
            ],
            ..Default::default()
        };
        let input = InputState::default().begin_pass(raw, false, 1.0, Default::default());
        assert_eq!(actions(&input), vec![ScrollLines(1), ScrollLines(-1)]);
    }

    #[test]
    fn repeats_scroll_but_do_not_toggle() {
        // `begin_pass` recomputes `repeat` from key state, so feed the second press in a new pass.
        let pass = |s: InputState, k| {
            s.begin_pass(
                RawInput {
                    events: vec![key(k, true, false)],
                    ..Default::default()
                },
                false,
                1.0,
                Default::default(),
            )
        };
        let s = pass(pass(InputState::default(), Key::J), Key::J);
        assert_eq!(actions(&s), vec![ScrollLines(1)]);
        let s = pass(pass(InputState::default(), Key::T), Key::T);
        assert_eq!(actions(&s), vec![]);
    }
}
