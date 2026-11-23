//! Fixed surfaces and app decisions. The library never owns this state.

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use ratatui_mouse::{
    HitAreas, LineSurface, MouseController, MouseOutcome, MouseReport, MouseSettings,
    ScrollPosition, SelectableSurface, SurfaceSet, Vt100Surface,
};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Left,
    Right,
    Tab,
    Divider,
}

impl Target {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Tab => "tab",
            Self::Divider => "divider",
        }
    }
}

pub struct App {
    pub controller: MouseController<Target>,
    pub areas: HitAreas<Target>,
    pub lines: Vec<String>,
    pub wrapped: Vec<bool>,
    pub scroll: ScrollPosition,
    pub parser: vt100::Parser,
    pub focused: Target,
    pub request_index: u8,
    pub is_alternate: bool,
    pub should_quit: bool,
}

impl App {
    pub fn new(settings: MouseSettings) -> Self {
        Self {
            controller: MouseController::new(settings),
            areas: HitAreas::new(),
            lines: create_lines(),
            wrapped: vec![false, false, false, false, false, true, true, false],
            scroll: ScrollPosition::default(),
            parser: vt100::Parser::new(10, 30, 200),
            focused: Target::Left,
            request_index: 0,
            is_alternate: false,
            should_quit: false,
        }
    }

    pub fn set_layout(&mut self, area: Rect) {
        self.areas.clear();
        let height = area.height.saturating_sub(3);
        let width = area.width.saturating_sub(5) / 2;
        self.areas.push(
            Rect::new(
                area.x.saturating_add(1),
                area.y.saturating_add(2),
                width,
                height,
            ),
            Target::Left,
        );
        self.areas.push(
            Rect::new(
                area.x.saturating_add(width).saturating_add(4),
                area.y.saturating_add(2),
                width,
                height,
            ),
            Target::Right,
        );
        self.areas.push(
            Rect::new(area.x, area.y, area.width, 1.min(area.height)),
            Target::Tab,
        );
        self.areas.push(
            Rect::new(
                area.x.saturating_add(width).saturating_add(2),
                area.y.saturating_add(1),
                1,
                area.height.saturating_sub(2),
            ),
            Target::Divider,
        );
        let is_initial = self.parser.screen().contents().is_empty();
        self.parser
            .screen_mut()
            .set_size(height.max(1), width.max(1));
        if is_initial {
            fill_parser(&mut self.parser);
        }
    }

    pub fn left_surface(&mut self) -> LineSurface<'_, String> {
        LineSurface::new(
            &self.lines,
            &mut self.scroll,
            self.areas
                .area_of(&Target::Left)
                .map_or(1, |area| area.height),
        )
        .with_wrapped_rows(&self.wrapped)
    }

    pub fn with_surfaces<R>(
        &mut self,
        operation: impl FnOnce(&mut MouseController<Target>, &HitAreas<Target>, &mut Surfaces<'_>) -> R,
    ) -> R {
        let height = self
            .areas
            .area_of(&Target::Left)
            .map_or(1, |area| area.height);
        let mut surfaces = Surfaces {
            left: LineSurface::new(&self.lines, &mut self.scroll, height)
                .with_wrapped_rows(&self.wrapped),
            right: Vt100Surface::new(self.parser.screen_mut()),
        };
        operation(&mut self.controller, &self.areas, &mut surfaces)
    }

    pub fn handle_event(&mut self, event: &Event, now: Instant) -> MouseReport<Target> {
        let was_selecting = self.controller.select_mode_target().is_some();
        let mut report = self.with_surfaces(|controller, areas, surfaces| {
            controller.handle_event(event, now, areas, surfaces)
        });
        if let Some(target @ (Target::Left | Target::Right)) = report.pressed_target {
            report.is_redraw_due |= self.focused != target;
            self.focused = target;
        }
        if let MouseOutcome::Wheel { target, rows, .. } = &report.outcome {
            self.scroll_target(*target, *rows);
            report.is_redraw_due = true;
        }
        if !was_selecting
            && let Event::Key(key) = event
            && key.kind != KeyEventKind::Release
        {
            report.is_redraw_due |= self.handle_key(key.code, key.modifiers);
            if key.code == KeyCode::Char('s') {
                let target = self.focused;
                report = self.with_surfaces(|controller, areas, surfaces| {
                    controller.enter_select_mode(target, None, areas, surfaces)
                });
            }
        }
        report
    }

    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        match code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            KeyCode::Char('!') => panic!("mouse_lab requested panic"),
            KeyCode::Tab => {
                self.focused = if self.focused == Target::Left {
                    Target::Right
                } else {
                    Target::Left
                }
            }
            KeyCode::Char('m') => {
                self.request_index = (self.request_index + 1) % 4;
                self.update_parser_modes();
            }
            KeyCode::Char('a') => {
                self.is_alternate = !self.is_alternate;
                self.update_parser_modes();
            }
            KeyCode::PageUp => self.scroll_target(self.focused, -10),
            KeyCode::PageDown => self.scroll_target(self.focused, 10),
            _ => return false,
        }
        true
    }

    pub fn parser_mode_sequence(&self) -> String {
        let mouse = match self.request_index {
            1 => "\x1b[?1000h\x1b[?1006h",
            2 => "\x1b[?1002h\x1b[?1006h",
            3 => "\x1b[?1000h",
            _ => "",
        };
        let alternate = if self.is_alternate {
            "\x1b[?1049h"
        } else {
            "\x1b[?1049l"
        };
        format!("\x1b[?1000l\x1b[?1002l\x1b[?1006l{mouse}{alternate}")
    }

    fn update_parser_modes(&mut self) {
        self.parser.process(self.parser_mode_sequence().as_bytes());
    }

    fn scroll_target(&mut self, target: Target, rows: i32) {
        self.with_surfaces(|_, _, surfaces| {
            if let Some(surface) = surfaces.surface_mut(&target) {
                surface.scroll_by(rows);
            }
        });
    }

    pub fn tick(&mut self, now: Instant) -> MouseReport<Target> {
        self.with_surfaces(|controller, areas, surfaces| controller.tick(now, areas, surfaces))
    }

    pub fn right_surface(&mut self) -> Vt100Surface<'_> {
        Vt100Surface::new(self.parser.screen_mut())
    }
}

pub struct Surfaces<'a> {
    left: LineSurface<'a, String>,
    right: Vt100Surface<'a>,
}
impl SurfaceSet<Target> for Surfaces<'_> {
    fn surface_mut(&mut self, target: &Target) -> Option<&mut dyn SelectableSurface> {
        match target {
            Target::Left => Some(&mut self.left),
            Target::Right => Some(&mut self.right),
            _ => None,
        }
    }
}

fn create_lines() -> Vec<String> {
    let mut lines: Vec<String> = [
        "alpha",
        "beta",
        "gamma",
        "file.rs:12",
        "中文 wide cells",
        "soft wrap ",
        "over three ",
        "rows",
        "a \"quote\" and punctuation",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    lines.extend((0..120).map(|line| format!("left history {line:03}")));
    lines
}

fn fill_parser(parser: &mut vt100::Parser) {
    for line in 0..100 {
        parser.process(format!("right history {line:03} 界\r\n").as_bytes());
    }
    parser.process(b"a fixed wrapped terminal line that extends across the entire viewport and into a second row\r\nlive terminal");
    parser.screen_mut().set_scrollback(usize::MAX);
}
