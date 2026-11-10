use crate::machine::connection;
use std::{
    collections::VecDeque,
    io::{Read, Write},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Default)]
pub struct MachineStatus {
    pub state: String,
    pub machine_position: Option<[f64; 3]>,
    pub work_offset: Option<[f64; 3]>,
    pub work_position: Option<[f64; 3]>,
    pub feed_rate: Option<f64>,
    pub spindle_rpm: Option<f64>,
}

#[derive(Debug)]
pub enum MachineEvent {
    Log(String),
    Status(MachineStatus),
    ProbeResult { position: [f64; 3], success: bool },
    Progress { sent: usize, total: usize },
    Finished,
    Disconnected(String),
}

enum Command {
    SendLine(String),
    StartJog { axis: char, direction: f64 },
    Stream(Vec<String>),
    Stop,
    CancelJog,
    Realtime(Vec<u8>),
    Disconnect,
}

pub struct MachineController {
    sender: Sender<Command>,
    pub events: Receiver<MachineEvent>,
    pub port_name: String,
}

impl MachineController {
    pub fn connect(port_name: String, baud_rate: u32) -> Result<Self, String> {
        let port = connection::open_port(&port_name, baud_rate)?;
        let (sender, commands) = mpsc::channel();
        let (events, receiver) = mpsc::channel();
        let worker_name = port_name.clone();
        thread::Builder::new()
            .name("tekkcam-serial".into())
            .spawn(move || run_worker(port, commands, events, worker_name))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            sender,
            events: receiver,
            port_name,
        })
    }

    pub fn send_line(&self, line: String) {
        let _ = self.sender.send(Command::SendLine(line));
    }
    pub fn start_jog(&self, axis: char, direction: f64) {
        let _ = self.sender.send(Command::StartJog { axis, direction });
    }
    pub fn stream(&self, gcode: &str) {
        let lines = gcode
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('%'))
            .map(str::to_owned)
            .collect();
        let _ = self.sender.send(Command::Stream(lines));
    }
    pub fn stop(&self) {
        let _ = self.sender.send(Command::Stop);
    }
    pub fn cancel_jog(&self) {
        let _ = self.sender.send(Command::CancelJog);
    }
    pub fn realtime(&self, bytes: Vec<u8>) {
        let _ = self.sender.send(Command::Realtime(bytes));
    }
    pub fn disconnect(&self) {
        let _ = self.sender.send(Command::Disconnect);
    }
}

impl Drop for MachineController {
    fn drop(&mut self) {
        let _ = self.sender.send(Command::Disconnect);
    }
}

fn run_worker(
    mut port: Box<dyn serialport::SerialPort>,
    commands: Receiver<Command>,
    events: Sender<MachineEvent>,
    port_name: String,
) {
    let mut lines = VecDeque::new();
    let mut total = 0;
    let mut sent = 0;
    let mut awaiting_ack = false;
    let mut stream_active = false;
    let mut awaiting_idle = false;
    let mut continuous_jog: Option<String> = None;
    let mut input = Vec::new();
    let mut read_buf = [0u8; 256];
    let mut last_status_query = Instant::now();

    let _ = events.send(MachineEvent::Log(format!("Connected to {port_name}")));
    loop {
        for command in commands.try_iter() {
            let result = match command {
                Command::SendLine(line) => {
                    if stream_active || continuous_jog.is_some() {
                        let _ = events.send(MachineEvent::Log(
                            "Command ignored while a job or continuous jog is active".into(),
                        ));
                        Ok(())
                    } else {
                        write_line(&mut *port, &line).map(|()| {
                            let _ = events.send(MachineEvent::Log(format!("→ {line}")));
                        })
                    }
                }
                Command::StartJog { axis, direction } => {
                    if stream_active {
                        let _ = events.send(MachineEvent::Log(
                            "Jog ignored while a job is streaming".into(),
                        ));
                        Ok(())
                    } else {
                        let cancel = if continuous_jog.is_some() {
                            port.write_all(&[0x85]).map_err(|error| error.to_string())
                        } else {
                            Ok(())
                        };
                        cancel.and_then(|()| {
                            let step = direction.signum();
                            let line = format!("$J=G91 {axis}{step:.0} F800");
                            continuous_jog = Some(line.clone());
                            write_line(&mut *port, &line).map(|()| {
                                let _ = events.send(MachineEvent::Log(format!(
                                    "Continuous jog started: {axis}"
                                )));
                            })
                        })
                    }
                }
                Command::Stream(job) => {
                    if stream_active || continuous_jog.is_some() {
                        let _ = events.send(MachineEvent::Log(
                            "A G-code stream is already active".into(),
                        ));
                    } else if job.is_empty() {
                        let _ = events.send(MachineEvent::Log("✖ No G-code lines to send".into()));
                    } else {
                        total = job.len();
                        sent = 0;
                        awaiting_ack = false;
                        awaiting_idle = false;
                        lines = job.into();
                        stream_active = true;
                        let _ = events
                            .send(MachineEvent::Log(format!("Streaming {total} G-code lines")));
                    }
                    Ok(())
                }
                Command::Stop => {
                    lines.clear();
                    stream_active = false;
                    awaiting_ack = false;
                    awaiting_idle = false;
                    continuous_jog = None;
                    port.write_all(&[b'!', 0x85])
                        .map(|()| {
                            let _ = events.send(MachineEvent::Log(
                                "Feed hold sent; pending G-code cleared".into(),
                            ));
                        })
                        .map_err(|error| error.to_string())
                }
                Command::CancelJog => {
                    continuous_jog = None;
                    port.write_all(&[0x85]).map_err(|error| error.to_string())
                }
                Command::Realtime(bytes) => {
                    port.write_all(&bytes).map_err(|error| error.to_string())
                }
                Command::Disconnect => {
                    let _ = events.send(MachineEvent::Disconnected("Disconnected".into()));
                    return;
                }
            };
            if let Err(error) = result {
                let _ = events.send(MachineEvent::Disconnected(format!(
                    "Serial write failed: {error}"
                )));
                return;
            }
        }

        if stream_active && !awaiting_ack {
            if let Some(line) = lines.pop_front() {
                if let Err(error) = write_line(&mut *port, &line) {
                    let _ = events.send(MachineEvent::Disconnected(format!(
                        "Serial write failed: {error}"
                    )));
                    return;
                }
                awaiting_ack = true;
                let _ = events.send(MachineEvent::Log(format!("→ {line}")));
            }
        }

        if last_status_query.elapsed() >= Duration::from_millis(250) {
            if let Err(error) = port.write_all(b"?") {
                let _ = events.send(MachineEvent::Disconnected(format!(
                    "Status query failed: {error}"
                )));
                return;
            }
            last_status_query = Instant::now();
        }

        match port.read(&mut read_buf) {
            Ok(count) => {
                for byte in &read_buf[..count] {
                    if *byte == b'\n' {
                        let line = String::from_utf8_lossy(&input).trim().to_owned();
                        input.clear();
                        if line == "ok" {
                            if stream_active && awaiting_ack {
                                sent += 1;
                                awaiting_ack = false;
                                let _ = events.send(MachineEvent::Progress { sent, total });
                                if lines.is_empty() && sent >= total {
                                    awaiting_idle = true;
                                    let _ = events.send(MachineEvent::Log(
                                        "All G-code lines accepted; waiting for machine idle"
                                            .into(),
                                    ));
                                }
                            } else if let Some(line) = continuous_jog.as_deref() {
                                if let Err(error) = write_line(&mut *port, line) {
                                    let _ = events.send(MachineEvent::Disconnected(format!(
                                        "Jog write failed: {error}"
                                    )));
                                    return;
                                }
                            }
                            let _ = events.send(MachineEvent::Log("← ok".into()));
                        } else if line.starts_with("error:") || line.starts_with("ALARM:") {
                            stream_active = false;
                            lines.clear();
                            awaiting_ack = false;
                            awaiting_idle = false;
                            let _ =
                                events.send(MachineEvent::Log(format!("← {line}; stream stopped")));
                        } else if let Some((position, success)) = parse_probe_result(&line) {
                            let _ = events.send(MachineEvent::ProbeResult { position, success });
                        } else if let Some(status) = parse_status(&line) {
                            if awaiting_idle && status.state.starts_with("Idle") {
                                awaiting_idle = false;
                                stream_active = false;
                                let _ = events.send(MachineEvent::Finished);
                            }
                            let _ = events.send(MachineEvent::Status(status));
                        } else if !line.is_empty() {
                            let _ = events.send(MachineEvent::Log(format!("← {line}")));
                        }
                    } else if *byte != b'\r' && input.len() < 4096 {
                        input.push(*byte);
                    }
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) => {}
            Err(error) => {
                let _ = events.send(MachineEvent::Disconnected(format!(
                    "Serial read failed: {error}"
                )));
                return;
            }
        }
    }
}

fn parse_probe_result(line: &str) -> Option<([f64; 3], bool)> {
    let body = line.strip_prefix("[PRB:")?.strip_suffix(']')?;
    let (coordinates, result) = body.rsplit_once(':')?;
    let position = parse_triplet(coordinates)?;
    let success = match result {
        "1" => true,
        "0" => false,
        _ => return None,
    };
    Some((position, success))
}

fn write_line(port: &mut dyn serialport::SerialPort, line: &str) -> Result<(), String> {
    port.write_all(line.as_bytes())
        .map_err(|error| error.to_string())?;
    port.write_all(b"\n").map_err(|error| error.to_string())?;
    port.flush().map_err(|error| error.to_string())
}

fn parse_status(line: &str) -> Option<MachineStatus> {
    let body = line.strip_prefix('<')?.strip_suffix('>')?;
    let mut fields = body.split('|');
    let mut status = MachineStatus {
        state: fields.next()?.to_owned(),
        ..Default::default()
    };
    for field in fields {
        if let Some(value) = field.strip_prefix("MPos:") {
            status.machine_position = parse_triplet(value);
        } else if let Some(value) = field.strip_prefix("WCO:") {
            status.work_offset = parse_triplet(value);
        } else if let Some(value) = field.strip_prefix("WPos:") {
            status.work_position = parse_triplet(value);
        } else if let Some(value) = field.strip_prefix("FS:") {
            let mut values = value.split(',').filter_map(|n| n.parse::<f64>().ok());
            status.feed_rate = values.next();
            status.spindle_rpm = values.next();
        } else if let Some(value) = field.strip_prefix("F:") {
            status.feed_rate = value.parse().ok();
        }
    }
    if status.work_position.is_none() {
        if let (Some(machine), Some(offset)) = (status.machine_position, status.work_offset) {
            status.work_position = Some([
                machine[0] - offset[0],
                machine[1] - offset[1],
                machine[2] - offset[2],
            ]);
        }
    }
    Some(status)
}

fn parse_triplet(value: &str) -> Option<[f64; 3]> {
    let mut values = value.split(',').map(str::parse::<f64>);
    Some([
        values.next()?.ok()?,
        values.next()?.ok()?,
        values.next()?.ok()?,
    ])
}

#[cfg(test)]
mod probing_tests {
    use super::parse_probe_result;

    #[test]
    fn parses_grbl_probe_position_and_trigger_status() {
        assert_eq!(
            parse_probe_result("[PRB:1.0,-2.5,3.25:1]"),
            Some(([1.0, -2.5, 3.25], true))
        );
        assert_eq!(
            parse_probe_result("[PRB:1,2,3:0]"),
            Some(([1.0, 2.0, 3.0], false))
        );
        assert_eq!(parse_probe_result("[PRB:1,2:1]"), None);
    }
}
