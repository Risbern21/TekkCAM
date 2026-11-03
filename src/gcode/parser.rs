use crate::cam::toolpath::ToolMove;

/// Parse the modal motion subset emitted by tekkCAM and commonly used in GRBL files.
/// Coordinates are returned in millimeters and absolute machine-independent XY positions.
pub fn parse(text: &str) -> Result<Vec<ToolMove>, String> {
    let mut moves = Vec::new();
    let (mut x, mut y, mut z) = (0.0, 0.0, 0.0);
    let mut coordinate_offset = [0.0; 3];
    let (mut absolute, mut absolute_arc_center, mut plane, mut scale, mut feed) =
        (true, false, 17, 1.0, 100.0);
    let mut motion = 0;

    for (line_index, original) in text.lines().enumerate() {
        let line_number = line_index + 1;
        let clean = strip_comments(original);
        let words = lex_words(&clean).map_err(|e| format!("Line {line_number}: {e}"))?;
        let mut axes = [None; 3];
        let mut ij = [None; 2];
        let mut radius = None;
        let mut reset_coordinates = false;
        let mut cancel_coordinate_offset = false;
        for (letter, value) in words {
            match letter {
                'G' if (value - 92.0).abs() < 1e-6 => reset_coordinates = true,
                'G' if (value - 92.1).abs() < 1e-6 => cancel_coordinate_offset = true,
                'G' if (value - 90.1).abs() < 1e-6 => absolute_arc_center = true,
                'G' if (value - 91.1).abs() < 1e-6 => absolute_arc_center = false,
                'G' => match value.round() as i32 {
                    0 => motion = 0,
                    1 => motion = 1,
                    2 => motion = 2,
                    3 => motion = 3,
                    17..=19 => plane = value.round() as i32,
                    20 => scale = 25.4,
                    21 => scale = 1.0,
                    90 => absolute = true,
                    91 => absolute = false,
                    // Feed mode and work offsets do not alter XY parsing.
                    54..=59 | 93..=95 => {}
                    _ => {}
                },
                'X' => axes[0] = Some(value * scale),
                'Y' => axes[1] = Some(value * scale),
                'Z' => axes[2] = Some(value * scale),
                'I' => ij[0] = Some(value * scale),
                'J' => ij[1] = Some(value * scale),
                'R' => radius = Some(value),
                'F' => feed = (value * scale).max(0.001),
                _ => {}
            }
        }
        if cancel_coordinate_offset {
            coordinate_offset = [0.0; 3];
        }
        if reset_coordinates {
            for axis in 0..3 {
                if let Some(programmed) = axes[axis] {
                    let machine = [x, y, z][axis];
                    coordinate_offset[axis] = machine - programmed;
                }
            }
            continue;
        }
        if axes.iter().all(Option::is_none) {
            continue;
        }
        let next = |old: f64, value: Option<f64>, offset: f64| match value {
            Some(v) if absolute => v + offset,
            Some(v) => old + v,
            None => old,
        };
        let (nx, ny, nz) = (
            next(x, axes[0], coordinate_offset[0]),
            next(y, axes[1], coordinate_offset[1]),
            next(z, axes[2], coordinate_offset[2]),
        );
        let movement = match motion {
            0 => ToolMove::Rapid {
                to: [nx, ny],
                z: nz,
            },
            1 => ToolMove::Feed {
                to: [nx, ny],
                z: nz,
                feed_mm_min: feed,
            },
            2 | 3 => {
                if plane != 17 {
                    return Err(format!(
                        "Line {line_number}: only G17 (XY plane) arcs are supported"
                    ));
                }
                if radius.is_some() {
                    return Err(format!(
                        "Line {line_number}: R-format arcs are not supported; use I/J offsets"
                    ));
                }
                let (Some(i), Some(j)) = (ij[0], ij[1]) else {
                    return Err(format!(
                        "Line {line_number}: arc requires I and J center offsets"
                    ));
                };
                let center_offset = if absolute_arc_center {
                    [i + coordinate_offset[0] - x, j + coordinate_offset[1] - y]
                } else {
                    [i, j]
                };
                ToolMove::Arc {
                    to: [nx, ny],
                    center_offset,
                    z: nz,
                    clockwise: motion == 2,
                    feed_mm_min: feed,
                }
            }
            _ => unreachable!(),
        };
        moves.push(movement);
        (x, y, z) = (nx, ny, nz);
    }
    if moves.is_empty() {
        return Err("No motion commands found in G-code".into());
    }
    Ok(moves)
}

fn strip_comments(line: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for ch in line.split('*').next().unwrap_or_default().chars() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ';' if depth == 0 => break,
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

fn lex_words(line: &str) -> Result<Vec<(char, f64)>, String> {
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut words = Vec::new();
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() || bytes[i] == b'%' {
            i += 1;
            continue;
        }
        let letter = bytes[i].to_ascii_uppercase() as char;
        if !letter.is_ascii_alphabetic() {
            return Err(format!("unexpected character '{}'", bytes[i] as char));
        }
        i += 1;
        let start = i;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        let mut digits = false;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
            digits |= bytes[i].is_ascii_digit();
            i += 1;
        }
        if !digits {
            return Err(format!("missing numeric value after {letter}"));
        }
        let value = line[start..i]
            .parse::<f64>()
            .map_err(|_| format!("invalid number after {letter}"))?;
        if !value.is_finite() {
            return Err(format!("non-finite number after {letter}"));
        }
        words.push((letter, value));
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g92_changes_coordinate_mapping_without_creating_motion() {
        let moves = parse("G21 G90\nG1 X10 F100\nG92 X0\nG1 X5\nG92.1\nG1 X0\n").unwrap();
        assert_eq!(moves.len(), 3);
        assert_eq!(moves[0].end(), [10.0, 0.0]);
        assert_eq!(moves[1].end(), [15.0, 0.0]);
        assert_eq!(moves[2].end(), [0.0, 0.0]);
    }
}
