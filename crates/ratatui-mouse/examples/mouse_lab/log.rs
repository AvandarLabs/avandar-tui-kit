//! Flushed, sequenced JSONL observability, private to the laboratory.
use std::io::{self, Write};

use crossterm::event::Event;
use ratatui_mouse::{HitAreas, MouseOutcome, MouseReport, PointerPress, Selection};
use serde_json::{Value, json};

use super::app::Target;

pub struct Log<W> {
    writer: W,
    sequence: u64,
}
impl<W: Write> Log<W> {
    pub const fn new(writer: W) -> Self {
        Self {
            writer,
            sequence: 0,
        }
    }
    pub fn ready(&mut self, marker: &std::path::Path) -> io::Result<()> {
        self.record(json!({"kind":"ready"}))?;
        let mut file = std::fs::File::create(marker)?;
        file.write_all(b"ready\n")?;
        file.flush()
    }
    pub fn record(&mut self, mut value: Value) -> io::Result<()> {
        self.sequence += 1;
        value["sequence"] = json!(self.sequence);
        serde_json::to_writer(&mut self.writer, &value)?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()
    }
}

pub fn report_record(report: &MouseReport<Target>) -> Value {
    json!({"kind":"report", "outcome":outcome_record(&report.outcome), "pressed_target":report.pressed_target.map(Target::name), "is_redraw_due":report.is_redraw_due})
}

pub fn layout_record(areas: &HitAreas<Target>) -> Value {
    let areas: Vec<Value> = areas.iter().map(|(area,target)|json!({"target":target.name(),"x":area.x,"y":area.y,"width":area.width,"height":area.height})).collect();
    json!({"kind":"layout","areas":areas})
}

pub fn event_record(event: &Event) -> Value {
    let event = match event {
        Event::Mouse(mouse) => {
            json!({"kind":"Mouse","mouse_kind":format!("{:?}",mouse.kind),"column":mouse.column,"row":mouse.row,"modifiers":format!("{:?}",mouse.modifiers)})
        }
        Event::Key(key) => {
            json!({"kind":"Key","code":format!("{:?}",key.code),"key_kind":format!("{:?}",key.kind),"modifiers":format!("{:?}",key.modifiers)})
        }
        Event::Resize(width, height) => json!({"kind":"Resize","width":width,"height":height}),
        _ => json!({"kind":format!("{event:?}")}),
    };
    json!({"kind":"event","event":event})
}

pub fn selection_record(selection: Option<&Selection<Target>>) -> Value {
    let selection = selection.map(|selection| {
        let range = selection.range();
        json!({"target":selection.target().name(), "range":{"start":{"line":range.start.line,"column":range.start.column},"end":{"line":range.end.line,"column":range.end.column}},"unit":format!("{:?}",selection.unit()),"stage":format!("{:?}",selection.stage())})
    });
    json!({"kind":"selection","selection":selection})
}

fn press_record(press: &PointerPress) -> Value {
    json!({"button":format!("{:?}",press.button),"position":{"x":press.position.x,"y":press.position.y},"content_point":press.content_point.map(|point| json!({"line":point.line,"column":point.column})),"clicks":press.clicks,"modifiers":format!("{:?}",press.modifiers)})
}

fn outcome_record(outcome: &MouseOutcome<Target>) -> Value {
    match outcome {
        MouseOutcome::Ignored => json!({"kind":"Ignored","target":null}),
        MouseOutcome::Consumed => json!({"kind":"Consumed","target":null}),
        MouseOutcome::Pressed { target, press } => {
            json!({"kind":"Pressed","target":target.name(),"press":press_record(press)})
        }
        MouseOutcome::Clicked { target, press } => {
            json!({"kind":"Clicked","target":target.name(),"press":press_record(press)})
        }
        MouseOutcome::Dragged {
            target,
            button,
            from,
            to,
        } => {
            json!({"kind":"Dragged","target":target.name(),"button":format!("{button:?}"),"from":{"x":from.x,"y":from.y},"to":{"x":to.x,"y":to.y}})
        }
        MouseOutcome::DragEnded {
            target,
            button,
            from,
            to,
        } => {
            json!({"kind":"DragEnded","target":target.name(),"button":format!("{button:?}"),"from":{"x":from.x,"y":from.y},"to":{"x":to.x,"y":to.y}})
        }
        MouseOutcome::Wheel {
            target,
            rows,
            columns,
            modifiers,
        } => {
            json!({"kind":"Wheel","target":target.name(),"rows":rows,"columns":columns,"modifiers":format!("{modifiers:?}")})
        }
        MouseOutcome::Copy { target, text } => {
            json!({"kind":"Copy","target":target.name(),"text":text})
        }
        MouseOutcome::Forward { target, bytes } => {
            json!({"kind":"Forward","target":target.name(),"bytes":bytes})
        }
        _ => json!({"kind":"Unknown","target":null}),
    }
}
