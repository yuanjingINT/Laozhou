use anyhow::Result;
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::style::{Attribute, Print, SetAttribute};
use crossterm::terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use std::io::{self, Write};
use std::time::Duration;

/// Visual state of the central voice orb.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrbState {
    /// Listening for the wake word or waiting for speech.
    Listening,
    /// User is recording.
    Recording,
    /// Assistant is thinking / generating.
    Thinking,
    /// Assistant is speaking (TTS playback).
    Speaking,
}

/// Full-screen voice assistant UI: a central water-wave orb on top with a
/// scrolling content area below. Supports space-to-wake and Ctrl+C to quit.
pub struct VoiceUi {
    active: bool,
    content_cursor: (u16, u16),
    content_top: u16,
}

impl VoiceUi {
    pub fn start() -> Result<Self> {
        terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, Hide)?;
        stdout.flush()?;
        let (_, rows) = terminal::size().unwrap_or((80, 24));
        let content_top = content_top_for(rows);
        Ok(Self {
            active: true,
            content_cursor: (0, content_top),
            content_top,
        })
    }

    pub fn finish(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        execute!(
            io::stdout(),
            Show,
            Clear(ClearType::All),
            LeaveAlternateScreen
        )?;
        terminal::disable_raw_mode()?;
        Ok(())
    }

    /// Non-blocking key check. Returns Ok(None) if nothing happened, Ok(Some(true))
    /// if space was pressed (manual wake), Ok(Some(false)) if quit was requested.
    pub fn poll_space(&mut self) -> Result<Option<bool>> {
        if event::poll(Duration::ZERO)? {
            match event::read()? {
                Event::Key(ev) if ev.code == KeyCode::Char(' ') => return Ok(Some(true)),
                Event::Key(ev)
                    if ev.code == KeyCode::Esc || ev.code == KeyCode::Char('q') =>
                {
                    return Ok(Some(false))
                }
                _ => {}
            }
        }
        Ok(None)
    }

    /// Draw one animation frame for the given state. Only the orb region is
    /// redrawn (no full clear), so the content area below is preserved.
    pub fn render(&mut self, state: OrbState, phase: f64, status: &str) -> Result<bool> {
        let (cols, rows) = terminal::size().unwrap_or((80, 24));
        let rows = rows.max(10);
        let cols = cols.max(20);
        let content_top = self.content_top.min(rows.saturating_sub(4));
        let orb_rows = content_top.saturating_sub(2).max(7);
        let orb_radius = (orb_rows / 2).saturating_sub(1).max(2);
        let orb_col = cols.saturating_sub(1) / 2;

        let mut stdout = io::stdout();
        let frame = build_frame(cols, rows, orb_rows, orb_col, orb_radius, state, phase);

        for (row, line) in frame.iter().enumerate() {
            execute!(
                stdout,
                MoveTo(0, row as u16),
                Clear(ClearType::CurrentLine),
                Print(line)
            )?;
        }
        // Status text under the orb.
        let status_row = orb_rows.saturating_add(1);
        if status_row < content_top {
            let status_col = orb_col.saturating_sub(status_line_width(status) / 2);
            execute!(
                stdout,
                MoveTo(0, status_row),
                Clear(ClearType::CurrentLine),
                MoveTo(status_col, status_row),
                SetAttribute(Attribute::Dim),
                Print(status),
                SetAttribute(Attribute::Reset)
            )?;
        }
        // Restore cursor to the content area so the next content frame is
        // written from the correct position.
        execute!(stdout, MoveTo(self.content_cursor.0, self.content_cursor.1))?;
        stdout.flush()?;
        Ok(false)
    }

    /// Begin a fresh content area: clear everything below the orb and reset the
    /// content cursor to the top of the content area.
    pub fn start_content(&mut self) -> Result<()> {
        let (_, rows) = terminal::size().unwrap_or((80, 24));
        let rows = rows.max(10);
        let content_top = self.content_top.min(rows.saturating_sub(4));
        self.content_cursor = (0, content_top);
        let mut stdout = io::stdout();
        for row in content_top..rows {
            execute!(
                stdout,
                MoveTo(0, row),
                Clear(ClearType::CurrentLine)
            )?;
        }
        execute!(stdout, MoveTo(0, content_top))?;
        stdout.flush()?;
        Ok(())
    }

    /// Render a streaming content frame into the lower content area. The frame
    /// is written verbatim (preserving all ANSI styling, tool calls, reasoning
    /// and command output) from the tracked content cursor, then the cursor is
    /// advanced by parsing the frame's terminal sequences.
    pub fn render_content(&mut self, frame: &[u8]) -> Result<()> {
        if frame.is_empty() {
            return Ok(());
        }
        let (cols, rows) = terminal::size().unwrap_or((80, 24));
        let rows = rows.max(10);
        let cols = cols.max(1);
        let content_top = self.content_top.min(rows.saturating_sub(4));
        let content_bottom = rows.saturating_sub(1);

        // Ensure the cursor stays within the content area.
        let cursor = self.content_cursor;
        let cursor = (
            cursor.0.min(cols.saturating_sub(1)),
            cursor.1.clamp(content_top, content_bottom),
        );

        let mut stdout = io::stdout();
        // Scroll region covers the content area so long content scrolls there.
        let region_top = (content_top + 1).min(rows);
        execute!(stdout, Print(format!("\x1b[{region_top};{rows}r")))?;
        execute!(stdout, MoveTo(cursor.0, cursor.1))?;
        stdout.write_all(frame)?;
        stdout.write_all(b"\x1b[r")?;
        stdout.flush()?;

        // Advance the tracked cursor by parsing the frame.
        let layout = crate::cli::terminal_frame_layout(frame, cursor, cols, Some(content_bottom));
        let next = layout.cursor;
        self.content_cursor = (
            next.0,
            next.1
                .max(content_top)
                .min(if next.1 >= content_bottom { content_bottom } else { next.1 }),
        );
        Ok(())
    }
}

impl Drop for VoiceUi {
    fn drop(&mut self) {
        if self.active {
            let _ = self.finish();
        }
    }
}

/// Characters used to draw the water-wave orb (dark → light).
const WAVE: [char; 6] = [' ', '░', '▒', '▓', '█', '█'];

#[allow(clippy::needless_range_loop)] // c is used for geometry (cx) and indexing.
fn build_frame(
    cols: u16,
    _rows: u16,
    orb_rows: u16,
    orb_col: u16,
    radius: u16,
    state: OrbState,
    phase: f64,
) -> Vec<String> {
    let cols = cols as usize;
    let mut grid: Vec<Vec<char>> = vec![vec![' '; cols]; orb_rows as usize];
    let radius = radius as f64;
    let center_col = orb_col as f64;
    let center_row = orb_rows as f64 / 2.0;

    for (r, row) in grid.iter_mut().enumerate() {
        let cy = r as f64 + 0.5;
        for c in 0..cols {
            let cx = c as f64 + 0.5;
            let dist = ((cx - center_col).powi(2) + (cy - center_row).powi(2)).sqrt();
            let dist_to_ring = (dist - radius).abs();
            if dist_to_ring < 0.9 {
                // Ring body: brightness waves with phase.
                let wave = (phase * std::f64::consts::TAU * 2.0 + dist).sin();
                let amp = match state {
                    OrbState::Listening => 0.35,
                    OrbState::Recording => 0.6,
                    OrbState::Thinking => 0.25,
                    // Speaking: strong pulsing wave driven by the "voice".
                    OrbState::Speaking => 0.9 + 0.1 * (phase * std::f64::consts::TAU * 10.0).sin(),
                };
                let idx = ((0.5 + 0.5 * (wave * amp)).clamp(0.0, 0.999) * 5.0) as usize;
                row[c] = WAVE[idx];
            } else if dist < radius {
                // Inner fill: for Speaking, the whole disc ripples like water
                // driven by a fast oscillating source at the center.
                let idx = match state {
                    OrbState::Thinking => 0,
                    _ => {
                        let center_wave =
                            (phase * std::f64::consts::TAU * 8.0 - dist * 0.8).sin();
                        let falloff = 1.0 - (dist / radius);
                        let mut brightness = falloff * (0.55 + 0.45 * center_wave);
                        if state == OrbState::Speaking {
                            brightness = (0.5 + 0.5 * center_wave) * falloff * 4.0;
                        }
                        ((brightness.clamp(0.0, 0.999) * 2.0) as usize).min(2)
                    }
                };
                row[c] = WAVE[idx];
            }
        }
    }

    // Speaking: outward sound ripples expanding from the center, like the
    // recording ripple but tied to the fast "voice" oscillation.
    if state == OrbState::Speaking {
        for i in 0..3 {
            let progress = (phase * 2.0 + i as f64 / 3.0).fract();
            let ripple_r = radius * 0.3 + progress * (radius + 3.0);
            for (r, row) in grid.iter_mut().enumerate() {
                let cy_d = r as f64 + 0.5;
                for c in 0..cols {
                    let cx_d = c as f64 + 0.5;
                    let d = ((cx_d - center_col).powi(2) + (cy_d - center_row).powi(2)).sqrt();
                    if (d - ripple_r).abs() < 0.45 && d > radius * 0.2 {
                        let idx = ((1.0 - progress) * 4.0) as usize + 1;
                        row[c] = WAVE[idx.min(5)];
                    }
                }
            }
        }
    }

    // Recording: expanding ripple rings.
    if state == OrbState::Recording {
        for i in 0..3 {
            let progress = (phase + i as f64 / 3.0).fract();
            let ripple_r = radius + progress * 4.0;
            for (r, row) in grid.iter_mut().enumerate() {
                let cy_d = r as f64 + 0.5;
                for c in 0..cols {
                    let cx_d = c as f64 + 0.5;
                    let d = ((cx_d - center_col).powi(2) + (cy_d - center_row).powi(2)).sqrt();
                    if (d - ripple_r).abs() < 0.45 && d > radius {
                        let idx = ((1.0 - progress) * 4.0) as usize + 1;
                        row[c] = WAVE[idx.min(5)];
                    }
                }
            }
        }
    }

    grid.into_iter()
        .map(|row| row.into_iter().collect())
        .collect()
}

/// The content area starts a few rows below the orb, clamped to a sane range.
pub fn content_top_for(rows: u16) -> u16 {
    rows.saturating_sub(6).clamp(10, 14)
}

fn status_line_width(status: &str) -> u16 {
    // Chinese and most text are width-1; count display columns conservatively.
    status.chars().count() as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_generates_for_all_states() {
        for state in [OrbState::Listening, OrbState::Recording, OrbState::Thinking, OrbState::Speaking] {
            let frame = build_frame(80, 24, 12, 40, 4, state, 0.5);
            assert_eq!(frame.len(), 12);
            for line in &frame {
                assert_eq!(line.chars().count(), 80);
            }
            // The orb must contain non-space characters.
            assert!(frame.iter().any(|l| l.chars().any(|c| c != ' ')));
        }
    }

    #[test]
    fn content_top_is_bounded() {
        assert_eq!(content_top_for(24), 14);
        assert_eq!(content_top_for(40), 14);
    }

    #[test]
    fn recording_state_draws_ripples() {
        let frame = build_frame(80, 24, 12, 40, 4, OrbState::Recording, 0.7);
        // Ripples extend beyond the base ring radius; many brightness cells.
        let wave_chars = frame
            .iter()
            .flat_map(|l| l.chars())
            .filter(|c| *c == '░' || *c == '▒')
            .count();
        assert!(wave_chars > 10, "Recording should draw expanding ripples");
    }
}




