use crate::model::{FULL_CHANCE, Note, TimeSignature};
use crate::time::PPQ;

#[derive(Debug, Clone, PartialEq)]
pub struct MidiClip {
    pub name: String,
    pub notes: Vec<Note>,
    pub length_tick: u64,
    pub tempo_bpm: Option<f64>,
}

fn push_varlen(out: &mut Vec<u8>, mut value: u32) {
    let mut bytes = vec![(value & 0x7f) as u8];
    value >>= 7;
    while value > 0 {
        bytes.push(((value & 0x7f) as u8) | 0x80);
        value >>= 7;
    }
    out.extend(bytes.iter().rev());
}

fn delta(tick: u64, last: &mut u64) -> u32 {
    let d = tick.saturating_sub(*last).min(0x0fff_ffff) as u32;
    *last = tick;
    d
}

pub fn write(clip: &MidiClip, tempo_bpm: f64, ts: TimeSignature) -> Vec<u8> {
    let mut events: Vec<(u64, u8, u8, u8)> = Vec::new();
    for note in &clip.notes {
        let end = note.start_tick + note.length_tick.max(1);
        events.push((
            note.start_tick,
            1,
            note.pitch.min(127),
            note.velocity.clamp(1, 127),
        ));
        events.push((end, 0, note.pitch.min(127), 0));
    }
    events.sort_by_key(|e| (e.0, e.1, e.2));
    let mut body = Vec::new();
    let mut last = 0;
    let name = clip.name.as_bytes();
    push_varlen(&mut body, 0);
    body.extend([0xff, 0x03]);
    push_varlen(&mut body, name.len() as u32);
    body.extend(name);
    let micros = (60_000_000.0 / tempo_bpm.max(1.0)).round() as u32;
    push_varlen(&mut body, 0);
    body.extend([0xff, 0x51, 0x03]);
    body.extend(&micros.to_be_bytes()[1..]);
    push_varlen(&mut body, 0);
    body.extend([
        0xff,
        0x58,
        0x04,
        ts.numerator.clamp(1, 255) as u8,
        ts.denominator.max(1).trailing_zeros() as u8,
        24,
        8,
    ]);
    for (tick, on, pitch, velocity) in events {
        push_varlen(&mut body, delta(tick, &mut last));
        body.extend([if on == 1 { 0x90 } else { 0x80 }, pitch, velocity]);
    }
    let end = clip.length_tick.max(last);
    push_varlen(&mut body, delta(end, &mut last));
    body.extend([0xff, 0x2f, 0x00]);
    let mut out = Vec::with_capacity(body.len() + 22);
    out.extend(b"MThd");
    out.extend(6u32.to_be_bytes());
    out.extend(0u16.to_be_bytes());
    out.extend(1u16.to_be_bytes());
    out.extend((PPQ as u16).to_be_bytes());
    out.extend(b"MTrk");
    out.extend((body.len() as u32).to_be_bytes());
    out.extend(body);
    out
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Result<u8, String> {
        let b = *self.data.get(self.at).ok_or("the file ends too early")?;
        self.at += 1;
        Ok(b)
    }

    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self.at.checked_add(n).filter(|e| *e <= self.data.len());
        let end = end.ok_or("the file ends too early")?;
        let slice = &self.data[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u16(&mut self) -> Result<u16, String> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn varlen(&mut self) -> Result<u32, String> {
        let mut value = 0u32;
        for _ in 0..4 {
            let b = self.byte()?;
            value = (value << 7) | u32::from(b & 0x7f);
            if b & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err("a length in the file is too long".into())
    }
}

pub fn read(data: &[u8]) -> Result<MidiClip, String> {
    let mut r = Reader { data, at: 0 };
    if r.take(4)? != b"MThd" {
        return Err("this is not a MIDI file".into());
    }
    let header_len = r.u32()? as usize;
    let header_end = r.at + header_len;
    let _format = r.u16()?;
    let tracks = r.u16()?;
    let division = r.u16()?;
    if division & 0x8000 != 0 || division == 0 {
        return Err("MIDI files timed in SMPTE frames are not supported".into());
    }
    r.at = header_end;
    let scale = |tick: u64| tick * PPQ / u64::from(division);
    let mut notes = Vec::new();
    let mut name = String::new();
    let mut tempo = None;
    let mut end_tick = 0;
    for _ in 0..tracks {
        if r.at + 8 > data.len() {
            break;
        }
        let kind = r.take(4)?.to_vec();
        let len = r.u32()? as usize;
        let chunk_end = r.at.checked_add(len).filter(|e| *e <= data.len());
        let chunk_end = chunk_end.ok_or("the file ends too early")?;
        if kind != b"MTrk" {
            r.at = chunk_end;
            continue;
        }
        let mut tick = 0u64;
        let mut status = 0u8;
        let mut open: Vec<(u8, u8, u64, u8)> = Vec::new();
        while r.at < chunk_end {
            tick += u64::from(r.varlen()?);
            let mut b = r.byte()?;
            if b == 0xff {
                let meta = r.byte()?;
                let len = r.varlen()? as usize;
                let body = r.take(len)?;
                match meta {
                    0x03 if name.is_empty() => name = String::from_utf8_lossy(body).into_owned(),
                    0x51 if body.len() == 3 && tempo.is_none() => {
                        let micros = u32::from_be_bytes([0, body[0], body[1], body[2]]);
                        if micros > 0 {
                            tempo = Some(60_000_000.0 / f64::from(micros));
                        }
                    }
                    _ => {}
                }
                continue;
            }
            if b == 0xf0 || b == 0xf7 {
                let len = r.varlen()? as usize;
                r.take(len)?;
                continue;
            }
            let first = if b & 0x80 == 0 {
                let data_byte = b;
                b = status;
                data_byte
            } else {
                status = b;
                r.byte()?
            };
            let channel = b & 0x0f;
            match b & 0xf0 {
                0x80 | 0x90 => {
                    let velocity = r.byte()?;
                    let pitch = first & 0x7f;
                    if b & 0xf0 == 0x90 && velocity > 0 {
                        open.push((channel, pitch, tick, velocity));
                    } else if let Some(i) = open
                        .iter()
                        .position(|(c, p, _, _)| *c == channel && *p == pitch)
                    {
                        let (_, _, start, velocity) = open.remove(i);
                        notes.push(Note {
                            pitch,
                            velocity: velocity.min(127),
                            start_tick: scale(start),
                            length_tick: scale(tick - start).max(1),
                            chance: FULL_CHANCE,
                        });
                    }
                }
                0xa0 | 0xb0 | 0xe0 => {
                    r.byte()?;
                }
                0xc0 | 0xd0 => {}
                _ => return Err("the file has an unknown event".into()),
            }
        }
        for (_, pitch, start, velocity) in open {
            notes.push(Note {
                pitch,
                velocity,
                start_tick: scale(start),
                length_tick: scale(tick.saturating_sub(start)).max(1),
                chance: FULL_CHANCE,
            });
        }
        end_tick = end_tick.max(scale(tick));
        r.at = chunk_end;
    }
    notes.sort_by_key(|n| (n.start_tick, n.pitch));
    let last = notes
        .iter()
        .map(|n| n.start_tick + n.length_tick)
        .max()
        .unwrap_or(0);
    let bar = PPQ * 4;
    let length = end_tick.max(last).max(1).div_ceil(bar) * bar;
    Ok(MidiClip {
        name,
        notes,
        length_tick: length,
        tempo_bpm: tempo,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(pitch: u8, start_tick: u64, length_tick: u64) -> Note {
        Note {
            pitch,
            velocity: 90,
            start_tick,
            length_tick,
            chance: FULL_CHANCE,
        }
    }

    #[test]
    fn written_files_read_back_the_same_notes() {
        let clip = MidiClip {
            name: "Bass".into(),
            notes: vec![
                note(36, 0, 480),
                note(48, 480, 960),
                note(36, 1920, 200_000),
            ],
            length_tick: PPQ * 4,
            tempo_bpm: None,
        };
        let ts = TimeSignature {
            numerator: 3,
            denominator: 4,
        };
        let bytes = write(&clip, 128.0, ts);
        assert_eq!(&bytes[..4], b"MThd");
        let back = read(&bytes).unwrap();
        assert_eq!(back.name, "Bass");
        assert_eq!(back.notes, clip.notes);
        assert!((back.tempo_bpm.unwrap() - 128.0).abs() < 0.01);
        assert_eq!(back.length_tick % (PPQ * 4), 0);
        assert!(back.length_tick >= 1920 + 200_000);
    }

    #[test]
    fn other_resolutions_and_running_status_are_read() {
        let mut body = vec![0x00, 0x90, 60, 100, 48, 60, 0, 0x00, 0xff, 0x2f, 0x00];
        let mut data = b"MThd".to_vec();
        data.extend(6u32.to_be_bytes());
        data.extend(0u16.to_be_bytes());
        data.extend(1u16.to_be_bytes());
        data.extend(96u16.to_be_bytes());
        data.extend(b"MTrk");
        data.extend((body.len() as u32).to_be_bytes());
        data.append(&mut body);
        let clip = read(&data).unwrap();
        assert_eq!(clip.notes, vec![note_with(60, 0, PPQ / 2, 100)]);
    }

    fn note_with(pitch: u8, start_tick: u64, length_tick: u64, velocity: u8) -> Note {
        Note {
            velocity,
            ..note(pitch, start_tick, length_tick)
        }
    }

    #[test]
    fn broken_files_are_errors() {
        assert!(read(b"nope").is_err());
        assert!(read(b"MThd\x00\x00\x00\x06\x00\x00\x00\x01").is_err());
        let mut data = write(
            &MidiClip {
                name: String::new(),
                notes: vec![note(60, 0, 10)],
                length_tick: 10,
                tempo_bpm: None,
            },
            120.0,
            TimeSignature {
                numerator: 4,
                denominator: 4,
            },
        );
        data.truncate(data.len() - 5);
        assert!(read(&data).is_err());
    }
}
