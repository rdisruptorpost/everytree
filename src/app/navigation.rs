use super::{Id, View, egui};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Visit {
    pub view: View,
    pub selected: Option<Id>,
}

#[derive(Default)]
pub(super) struct History {
    back: Vec<Visit>,
    forward: Vec<Visit>,
}

impl History {
    /// Record a new destination. Taking a different route discards redo history,
    /// just as following a new link does after going back in a browser.
    pub fn record(&mut self, current: Visit) {
        self.back.push(current);
        self.forward.clear();
    }
    pub fn can_back(&self) -> bool {
        !self.back.is_empty()
    }
    pub fn can_forward(&self) -> bool {
        !self.forward.is_empty()
    }
    pub fn back(&mut self, current: Visit) -> Option<Visit> {
        let destination = self.back.pop()?;
        self.forward.push(current);
        Some(destination)
    }
    pub fn forward(&mut self, current: Visit) -> Option<Visit> {
        let destination = self.forward.pop()?;
        self.back.push(current);
        Some(destination)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Direction {
    Back,
    Forward,
}

pub(super) fn input(ctx: &egui::Context) -> Option<Direction> {
    let typing = ctx.wants_keyboard_input();
    ctx.input(|i| {
        // Native XBUTTON1/2 arrive via egui-winit as Extra1/2. Handle them on
        // press, independent of text focus, rather than waiting for release.
        let back = i.pointer.button_pressed(egui::PointerButton::Extra1)
            || i.key_pressed(egui::Key::BrowserBack)
            || (i.modifiers.alt && i.key_pressed(egui::Key::ArrowLeft))
            || (!typing && !i.modifiers.shift && i.key_pressed(egui::Key::Backspace));
        let forward = i.pointer.button_pressed(egui::PointerButton::Extra2)
            || (i.modifiers.alt && i.key_pressed(egui::Key::ArrowRight))
            || (!typing && i.modifiers.shift && i.key_pressed(egui::Key::Backspace));
        // One navigation per frame, even if a mouse driver also sends a key.
        if back {
            Some(Direction::Back)
        } else if forward {
            Some(Direction::Forward)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visit(parent: Id, start: usize, selected: Option<Id>) -> Visit {
        Visit {
            view: View {
                parent,
                start,
                end: start + 10,
            },
            selected,
        }
    }

    #[test]
    fn round_trip_restores_folder_group_and_selection() {
        let root = visit(0, 0, None);
        let folder = visit(12, 0, Some(15));
        let group = visit(12, 30, Some(47));
        let mut history = History::default();
        history.record(root);
        history.record(folder);
        assert_eq!(history.back(group), Some(folder));
        assert_eq!(history.back(folder), Some(root));
        assert_eq!(history.back(root), None);
        assert_eq!(history.forward(root), Some(folder));
        assert_eq!(history.forward(folder), Some(group));
        assert_eq!(history.forward(group), None);
    }

    #[test]
    fn taking_a_new_route_discards_the_old_forward_branch() {
        let a = visit(1, 0, None);
        let b = visit(2, 0, None);
        let c = visit(3, 0, None);
        let mut history = History::default();
        history.record(a);
        assert_eq!(history.back(b), Some(a));
        assert!(history.can_forward());
        history.record(a); // Navigate from a to c instead of returning to b.
        assert!(!history.can_forward());
        assert_eq!(history.back(c), Some(a));
        assert_eq!(history.forward(a), Some(c));
    }

    #[test]
    fn mouse_side_buttons_navigate_on_press_not_hold_or_release() {
        for (button, expected) in [
            (egui::PointerButton::Extra1, Direction::Back),
            (egui::PointerButton::Extra2, Direction::Forward),
        ] {
            let ctx = egui::Context::default();
            let mut action = None;
            for pressed in [Some(true), None, Some(false)] {
                let events = pressed
                    .map(|pressed| egui::Event::PointerButton {
                        pos: egui::pos2(30.0, 30.0),
                        button,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    })
                    .into_iter()
                    .collect();
                let _ = ctx.run(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        action = input(ctx);
                    },
                );
                if pressed == Some(true) {
                    assert_eq!(
                        action,
                        Some(match expected {
                            Direction::Back => Direction::Back,
                            Direction::Forward => Direction::Forward,
                        })
                    );
                } else {
                    assert_eq!(action, None);
                }
            }
        }
    }
}
