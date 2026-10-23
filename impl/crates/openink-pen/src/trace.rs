//! The `pen-dump` text trace (v1): `#` header lines, then one `time_us type code value` per event.
//! Shared by the replay test and the stroke preview so the format has one parser.

use crate::decode::{AbsRange, RawEvent};

pub struct Trace {
    pub x: AbsRange,
    pub y: AbsRange,
    pub pressure: AbsRange,
    pub events: Vec<RawEvent>,
}

/// Parse a trace; errors name the offending line.
pub fn parse(text: &str) -> Result<Trace, String> {
    let mut ranges = None;
    let mut events = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let bad = |what: &str| format!("line {}: {what}: {line:?}", n + 1);
        if let Some(h) = line.strip_prefix("# x ") {
            // "0 20832 y 0 15624 pressure 0 4095 eraser true tilt true"
            let f: Vec<&str> = h.split_whitespace().collect();
            if f.len() < 8 || f[2] != "y" || f[5] != "pressure" {
                return Err(bad("header shape changed"));
            }
            let at = |i: usize| f[i].parse::<i32>().map_err(|_| bad("bad range"));
            let r = |a, b| -> Result<AbsRange, String> { Ok(AbsRange { min: at(a)?, max: at(b)? }) };
            ranges = Some((r(0, 1)?, r(3, 4)?, r(6, 7)?));
        } else if !line.starts_with('#') && !line.trim().is_empty() {
            let v: Vec<i64> = line.split_whitespace().map(str::parse).collect::<Result<_, _>>().map_err(|_| bad("not numbers"))?;
            let [t, kind, code, value] = v[..] else { return Err(bad("want 4 fields")) };
            events.push(RawEvent { time_us: t, kind: kind as u16, code: code as u16, value: value as i32 });
        }
    }
    let (x, y, pressure) = ranges.ok_or("no '# x' header")?;
    Ok(Trace { x, y, pressure, events })
}
