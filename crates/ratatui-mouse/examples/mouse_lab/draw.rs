//! Render fixed fixtures and patch selection geometry through the public API.
use super::app::{App, Target};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::{Block, Paragraph},
};
use ratatui_mouse::{DEFAULT_HIGHLIGHT_STYLE, HighlightRects, SelectableSurface};

pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    app.set_layout(frame.area());
    let title = format!(
        "mouse_lab | focus: {} | mouse: {} | alternate: {}",
        app.focused.name(),
        app.request_index,
        app.is_alternate
    );
    if let Some(area) = app.areas.area_of(&Target::Tab) {
        frame.render_widget(
            Paragraph::new(title).style(Style::new().fg(Color::Cyan)),
            area,
        );
    }
    for target in [Target::Left, Target::Right] {
        let Some(area) = app.areas.area_of(&target) else {
            continue;
        };
        draw_pane(frame, app, target, area);
    }
    if let Some(area) = app.areas.area_of(&Target::Divider) {
        frame.render_widget(Block::new().style(Style::new().bg(Color::DarkGray)), area);
    }
    let footer = Rect::new(
        frame.area().x,
        frame.area().bottom().saturating_sub(1),
        frame.area().width,
        1.min(frame.area().height),
    );
    frame.render_widget(
        Paragraph::new(
            "Tab focus | s select | m mouse | a alt | PgUp/Dn scroll | q quit | ! panic",
        ),
        footer,
    );
}

fn draw_pane(frame: &mut Frame<'_>, app: &mut App, target: Target, area: Rect) {
    let border = Rect::new(
        area.x.saturating_sub(1),
        area.y.saturating_sub(1),
        area.width.saturating_add(2),
        area.height.saturating_add(2),
    );
    frame.render_widget(Block::bordered().title(target.name()), border);
    let origin = if target == Target::Left {
        let lines: Vec<Line<'_>> = app
            .lines
            .iter()
            .skip(app.scroll.line)
            .take(usize::from(area.height))
            .map(|line| Line::raw(line.as_str()))
            .collect();
        frame.render_widget(Paragraph::new(lines), area);
        app.left_surface().origin()
    } else {
        let lines: Vec<Line<'_>> = app
            .parser
            .screen()
            .rows(0, area.width)
            .map(Line::raw)
            .collect();
        frame.render_widget(Paragraph::new(lines), area);
        app.right_surface().origin()
    };
    if let Some(selection) = app
        .controller
        .selection()
        .filter(|selection| *selection.target() == target)
    {
        HighlightRects::from_range(selection.range(), origin, area)
            .render(frame.buffer_mut(), DEFAULT_HIGHLIGHT_STYLE);
    }
    if app.controller.select_mode_target() == Some(&target)
        && let Some(position) = app
            .controller
            .select_mode_cursor()
            .and_then(|cursor| cursor.to_screen_position(area, origin))
    {
        frame.set_cursor_position(position);
    }
}
