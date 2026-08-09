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
}

impl VoiceUi {
    pub fn start() -> Result<Self> {
        terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, Hide)?;
        stdout.flush()?;
        Ok(Self { active: true })
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

    /// Draw one animation frame for the given state. `phase` advances the
    /// animation (0..1 repeating). Returns whether the terminal size changed.
    pub fn render(&mut self, state: OrbState, phase: f64, status: &str) -> Result<bool> {
        let (cols, rows) = terminal::size().unwrap_or((80, 24));
        let rows = rows.max(10);
        let cols = cols.max(20);
        let content_top = 14u16.min(rows.saturating_sub(6));
        let orb_rows = content_top.saturating_sub(2).max(7);
        let orb_radius = (orb_rows / 2).saturating_sub(1).max(2);
        let orb_col = cols.saturating_sub(1) / 2;

        let mut stdout = io::stdout();
        let frame = build_frame(cols, rows, orb_rows, orb_col, orb_radius, state, phase);
        let status_line = status;

        execute!(
            stdout,
            MoveTo(0, 0),
            Clear(ClearType::All),
            Hide
        )?;
        for (row, line) in frame.iter().enumerate() {
            execute!(stdout, MoveTo(0, row as u16), Print(line))?;
        }
        // Status text under the orb.
        let status_row = orb_rows.saturating_add(1);
        if status_row < rows {
            let status_col = orb_col.saturating_sub(status_line.chars().count() as u16 / 2);
            execute!(
                stdout,
                MoveTo(status_col, status_row),
                SetAttribute(Attribute::Dim),
                Print(status_line),
                SetAttribute(Attribute::Reset)
            )?;
        }
        stdout.flush()?;
        Ok(false)
    }

    /// Render a streaming content frame into the lower content area.
    pub fn render_content(&mut self, content_top: u16, frame: &[u8]) -> Result<()> {
        let (cols, rows) = terminal::size().unwrap_or((80, 24));
        let rows = rows.max(10);
        let content_top = content_top.min(rows.saturating_sub(2));
        let mut stdout = io::stdout();
        let text = String::from_utf8_lossy(frame);
        let mut lines: Vec<&str> = text.lines().collect();
        // Keep only the lines that fit below the orb.
        let max_lines = rows.saturating_sub(content_top).saturating_sub(1) as usize;
        if lines.len() > max_lines {
            lines = lines.split_off(lines.len() - max_lines);
        }
        let _ = cols;
        for (i, line) in lines.iter().enumerate() {
            let row = content_top + i as u16;
            if row >= rows {
                break;
            }
            execute!(
                stdout,
                MoveTo(0, row),
                Clear(ClearType::CurrentLine),
                Print(line)
            )?;
        }
        stdout.flush()?;
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

    for r in 0..orb_rows as usize {
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
                    OrbState::Speaking => 0.8 + 0.2 * (phase * std::f64::consts::TAU * 8.0).sin(),
                };
                let idx = ((0.5 + 0.5 * (wave * amp)).clamp(0.0, 0.999) * 5.0) as usize;
                grid[r][c] = WAVE[idx];
            } else if dist < radius {
                // Inner fill.
                let idx = match state {
                    OrbState::Thinking => 0,
                    _ => {
                        let brightness =
                            (1.0 - (dist / radius)) * (0.5 + 0.5 * (phase * std::f64::consts::TAU).sin());
                        ((brightness.clamp(0.0, 0.999) * 2.0) as usize).min(2)
                    }
                };
                grid[r][c] = WAVE[idx];
            }
        }
    }

    // Thinking: orbiting droplets.
    if state == OrbState::Thinking {
        let droplets = 5usize;
        for i in 0..droplets {
            let ang =
                phase * std::f64::consts::TAU * 2.0 + i as f64 * std::f64::consts::TAU / droplets as f64;
            let orbit = radius + 1.6;
            let x = (center_col + orbit * ang.cos()).round() as usize;
            let y = (center_row + orbit * ang.sin() * 0.45).round() as usize;
            if y < orb_rows as usize && x < cols {
                grid[y][x] = if grid[y][x] == ' ' { '●' } else { '◍' };
            }
        }
    }

    // Speaking: pulse dot at center.
    if state == OrbState::Speaking {
        let pulse = (phase * std::f64::consts::TAU * 8.0).sin() * 0.5 + 0.5;
        let cy = center_row.round() as usize;
        let cx = orb_col as usize;
        if cy < orb_rows as usize && cx < cols {
            grid[cy][cx] = if pulse > 0.6 { '◉' } else { '●' };
        }
    }

    // Recording: expanding ripple rings.
    if state == OrbState::Recording {
        for i in 0..3 {
            let progress = (phase + i as f64 / 3.0).fract();
            let ripple_r = radius + progress * 4.0;
            for r in 0..orb_rows as usize {
                let cy_d = r as f64 + 0.5;
                for c in 0..cols {
                    let cx_d = c as f64 + 0.5;
                    let d = ((cx_d - center_col).powi(2) + (cy_d - center_row).powi(2)).sqrt();
                    if (d - ripple_r).abs() < 0.45 && d > radius {
                        let idx = ((1.0 - progress) * 4.0) as usize + 1;
                        grid[r][c] = WAVE[idx.min(5)];
                    }
                }
            }
        }
    }

    grid.into_iter()
        .map(|row| row.into_iter().collect())
        .collect()
}

/// Keep the terminal size independent of content scroll: unused helper.
pub fn content_top_for(rows: u16) -> u16 {
    rows.saturating_sub(6).min(14).max(10)
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
    fn thinking_state_draws_droplets() {
        let frame = build_frame(80, 24, 12, 40, 4, OrbState::Thinking, 0.5);
        let has_droplet = frame
            .iter()
            .any(|l| l.contains('●') || l.contains('◍'));
        assert!(has_droplet, "Thinking should draw orbiting droplets");
    }

    #[test]
    fn speaking_state_draws_center_pulse() {
        let frame = build_frame(80, 24, 12, 40, 4, OrbState::Speaking, 0.3);
        let has_pulse = frame.iter().any(|l| l.contains('◉') || l.contains('●'));
        assert!(has_pulse, "Speaking should draw a center pulse");
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
