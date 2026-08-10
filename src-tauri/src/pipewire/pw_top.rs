//! Thin parser around `pw-top -b -n <iterations>`, PipeWire's batch-mode
//! per-node scheduling snapshot (see `man 1 pw-top`). Used for the
//! theoretical/buffering latency figure (issue #223) — QUANT/RATE gives a
//! buffering-latency number without linking libpipewire, the same
//! shell-out-friendly tradeoff the rest of `AudioBackend`'s Linux impl
//! already makes.
//!
//! A single iteration (`-n 1`) reports QUANT/RATE as 0 for every node,
//! confirmed against a real PipeWire 1.5.85 session — the first batch is
//! just the initial snapshot before any scheduling has happened. At least
//! two iterations are required for real numbers, and only nodes in the `R`
//! (running) state carry a usable reading; `S`/`I`/`C` rows report `---`
//! and must be treated as "no data", not zero latency.

use crate::backend::BackendError;
use crate::sysproc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PwTopRow {
    pub node_id: u32,
    pub quantum: Option<u32>,
    pub rate: Option<u32>,
}

/// Runs `pw-top -b -n <iterations>` and returns the last reported row per
/// node id (later batches are more settled than the first).
pub fn run_batch(iterations: u32) -> Result<Vec<PwTopRow>, BackendError> {
    let output = sysproc::command("pw-top")
        .arg("-b")
        .arg("-n")
        .arg(iterations.to_string())
        .output()
        .map_err(|error| BackendError::Message(format!("failed to run pw-top: {error}")))?;

    if !output.status.success() {
        return Err(BackendError::Message(
            "pw-top failed while measuring latency".to_string(),
        ));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_batch_output(&text))
}

/// Parses `pw-top -b` output, keeping the last occurrence of each node id
/// (the header row `S   ID  QUANT ...` repeats once per iteration; data rows
/// interleave a fresh reading per node each time).
fn parse_batch_output(text: &str) -> Vec<PwTopRow> {
    let mut rows: Vec<PwTopRow> = Vec::new();

    for line in text.lines() {
        let mut fields = line.split_whitespace();

        // First column is the state char (S/R/I/C/...); skip non-data rows
        // (the header line's first token is "S", one letter but not
        // immediately followed by a numeric ID column).
        let Some(state) = fields.next() else {
            continue;
        };
        if state.len() != 1 || !state.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
            continue;
        }

        let Some(Ok(node_id)) = fields.next().map(str::parse::<u32>) else {
            continue;
        };
        let quantum = fields.next().and_then(|token| token.parse::<u32>().ok());
        let rate = fields.next().and_then(|token| token.parse::<u32>().ok());

        let quantum = quantum.filter(|value| *value > 0);
        let rate = rate.filter(|value| *value > 0);

        if let Some(existing) = rows.iter_mut().find(|row| row.node_id == node_id) {
            existing.quantum = quantum;
            existing.rate = rate;
        } else {
            rows.push(PwTopRow {
                node_id,
                quantum,
                rate,
            });
        }
    }

    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured from a real `pw-top -b -n 2` run (PipeWire 1.5.85) — two
    /// iterations, header repeated, first iteration all-zero, second
    /// iteration has real QUANT/RATE for the two currently-running nodes
    /// (63, 105) and `---` for everything else.
    const SAMPLE: &str = "\
S   ID  QUANT   RATE    WAIT    BUSY   W/Q   B/Q  ERR FORMAT           NAME
C   30      0      0    ---     ---   ---   ---     0                  Dummy-Driver
C   31      0      0    ---     ---   ---   ---     0                  Freewheel-Driver
C   63      0      0    ---     ---   ---   ---     0                  alsa_output.pci-0000_01_00.1.hdmi-stereo
C  105      0      0    ---     ---   ---   ---     0                  cs2
S   ID  QUANT   RATE    WAIT    BUSY   W/Q   B/Q  ERR FORMAT           NAME
I   30      0      0   0.0us   0.0us  ???   ???     0                  Dummy-Driver
S   31      0      0    ---     ---   ---   ---     0                  Freewheel-Driver
R   63   1024  48000  62.9us   9.9us  0.00  0.00    0    S32LE 2 48000 alsa_output.pci-0000_01_00.1.hdmi-stereo
R  105   1024  44100  12.7us  44.7us  0.00  0.00    1    F32LE 2 44100  + cs2
";

    #[test]
    fn keeps_last_reading_per_node_id() {
        let rows = parse_batch_output(SAMPLE);

        let node_63 = rows.iter().find(|row| row.node_id == 63).expect("node 63");
        assert_eq!(node_63.quantum, Some(1024));
        assert_eq!(node_63.rate, Some(48000));

        let node_105 = rows
            .iter()
            .find(|row| row.node_id == 105)
            .expect("node 105");
        assert_eq!(node_105.quantum, Some(1024));
        assert_eq!(node_105.rate, Some(44100));
    }

    #[test]
    fn suspended_node_with_no_final_reading_has_no_data() {
        let rows = parse_batch_output(SAMPLE);

        let node_31 = rows.iter().find(|row| row.node_id == 31).expect("node 31");
        assert_eq!(node_31.quantum, None);
        assert_eq!(node_31.rate, None);
    }

    #[test]
    fn zero_quantum_and_rate_are_treated_as_no_data() {
        let rows = parse_batch_output("S   ID  QUANT   RATE    WAIT    BUSY   W/Q   B/Q  ERR FORMAT           NAME \nC   30      0      0    ---     ---   ---   ---     0                  Dummy-Driver\n");
        let node_30 = rows.iter().find(|row| row.node_id == 30).expect("node 30");
        assert_eq!(node_30.quantum, None);
        assert_eq!(node_30.rate, None);
    }

    #[test]
    fn ignores_header_and_blank_lines() {
        let rows = parse_batch_output(
            "S   ID  QUANT   RATE    WAIT    BUSY   W/Q   B/Q  ERR FORMAT           NAME \n\n",
        );
        assert!(rows.is_empty());
    }
}
