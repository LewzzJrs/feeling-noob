use crate::processor::{MidiEvent, midi_channel};
use midir::{Ignore, MidiInput, MidiInputConnection};
use rtrb::{Consumer, Producer};
use std::fmt;

const CLIENT_NAME: &str = "feeling-noob";
const PORT_NAME: &str = "feeling-noob input";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiInputInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug)]
pub enum MidiError {
    Init(String),
    NoPort,
    NotFound(String),
    Connect(String),
}

impl fmt::Display for MidiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Init(e) => write!(f, "MIDI is not available: {e}"),
            Self::NoPort => write!(f, "no MIDI input found"),
            Self::NotFound(id) => write!(f, "MIDI input not found: {id}"),
            Self::Connect(e) => write!(f, "could not open MIDI input: {e}"),
        }
    }
}

impl std::error::Error for MidiError {}

pub fn parse_message(message: &[u8]) -> Option<MidiEvent> {
    let (&status, data) = message.split_first()?;
    match (status & 0xF0, data) {
        (0x90, [pitch, velocity, ..]) => Some(MidiEvent {
            pitch: pitch & 0x7F,
            velocity: velocity & 0x7F,
        }),
        (0x80, [pitch, _, ..]) => Some(MidiEvent {
            pitch: pitch & 0x7F,
            velocity: 0,
        }),
        _ => None,
    }
}

fn new_input() -> Result<MidiInput, MidiError> {
    let mut input = MidiInput::new(CLIENT_NAME).map_err(|e| MidiError::Init(e.to_string()))?;
    input.ignore(Ignore::All);
    Ok(input)
}

pub fn list_midi_inputs() -> Result<Vec<MidiInputInfo>, MidiError> {
    let input = new_input()?;
    Ok(input
        .ports()
        .iter()
        .map(|port| MidiInputInfo {
            id: port.id(),
            name: input.port_name(port).unwrap_or_else(|_| port.id()),
        })
        .collect())
}

pub struct MidiConnection {
    _connection: MidiInputConnection<Producer<MidiEvent>>,
    pub name: String,
}

pub fn connect(port_id: Option<&str>) -> Result<(MidiConnection, Consumer<MidiEvent>), MidiError> {
    let input = new_input()?;
    let port = match port_id {
        Some(id) => input
            .find_port_by_id(id)
            .ok_or_else(|| MidiError::NotFound(id.to_owned()))?,
        None => input.ports().into_iter().next().ok_or(MidiError::NoPort)?,
    };
    let name = input.port_name(&port).unwrap_or_else(|_| port.id());
    let (tx, rx) = midi_channel();
    let connection = input
        .connect(
            &port,
            PORT_NAME,
            |_, message, tx: &mut Producer<MidiEvent>| {
                if let Some(event) = parse_message(message) {
                    let _ = tx.push(event);
                }
            },
            tx,
        )
        .map_err(|e| MidiError::Connect(e.to_string()))?;
    Ok((
        MidiConnection {
            _connection: connection,
            name,
        },
        rx,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(pitch: u8, velocity: u8) -> Option<MidiEvent> {
        Some(MidiEvent { pitch, velocity })
    }

    #[test]
    fn parses_note_on_and_off_on_any_channel() {
        assert_eq!(parse_message(&[0x90, 60, 100]), event(60, 100));
        assert_eq!(parse_message(&[0x9F, 61, 1]), event(61, 1));
        assert_eq!(parse_message(&[0x90, 62, 0]), event(62, 0));
        assert_eq!(parse_message(&[0x83, 63, 64]), event(63, 0));
    }

    #[test]
    fn ignores_other_and_short_messages() {
        assert_eq!(parse_message(&[]), None);
        assert_eq!(parse_message(&[0x90, 60]), None);
        assert_eq!(parse_message(&[0xB0, 64, 127]), None);
        assert_eq!(parse_message(&[0xE0, 0, 64]), None);
        assert_eq!(parse_message(&[0xF8]), None);
    }

    #[test]
    fn masks_data_bytes() {
        assert_eq!(parse_message(&[0x90, 0xBC, 0xFF]), event(60, 127));
    }
}
