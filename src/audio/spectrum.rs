use std::collections::VecDeque;
use std::f32::consts::PI;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use tokio::sync::mpsc::UnboundedSender;

pub const NUM_BARS: usize = 128;
const FFT_SIZE: usize = 2048;
const SAMPLE_RATE: f32 = 48000.0;

pub fn launch(viz_buf: Arc<Mutex<VecDeque<f32>>>, tx: UnboundedSender<Vec<f32>>) {
    std::thread::spawn(move || {
        let mut analyzer = Analyzer::new();
        // Frames sent since audio last arrived. Every frame makes the window
        // redraw, so after the bars have had time to fall to zero, nothing
        // more is sent until music plays again: an idle player stays idle.
        let mut silent_frames = 0u32;
        loop {
            std::thread::sleep(Duration::from_millis(33));

            let samples: Vec<f32> = {
                let mut buf = viz_buf.lock().unwrap();
                if buf.is_empty() {
                    silent_frames += 1;
                } else {
                    silent_frames = 0;
                }
                if silent_frames > SETTLE_FRAMES {
                    continue;
                }
                let n = buf.len().min(FFT_SIZE);
                let skip = buf.len().saturating_sub(FFT_SIZE);
                let v: Vec<f32> = buf.iter().skip(skip).copied().collect();
                buf.clear();
                // last n samples, zero-padded to FFT_SIZE on the left
                let mut out = vec![0.0f32; FFT_SIZE];
                let offset = FFT_SIZE - n;
                out[offset..].copy_from_slice(&v[v.len().saturating_sub(FFT_SIZE - offset)..]);
                out
            };

            if tx.send(analyzer.frame(&samples)).is_err() {
                break;
            }
        }
    });
}

/// Silent frames sent after the music stops, so the bars fall smoothly to
/// zero before the visualiser goes quiet (about a second).
const SETTLE_FRAMES: u32 = 30;

/// Turns ~33 ms of audio into bar heights (0‥1).
///
/// Heights are in decibels, not raw amplitude: music's highs are 30–40 dB
/// below its bass, so on a linear scale only the lowest bars ever moved.
/// A tilt of a few dB per octave evens out that natural slope, and the scale
/// is measured from the loudest recent moment, so a quiet verse sits visibly
/// lower than the chorus while a quiet song still fills the display after a
/// few seconds.
pub struct Analyzer {
    fft: std::sync::Arc<dyn rustfft::Fft<f32>>,
    scratch: Vec<Complex<f32>>,
    hann: Vec<f32>,
    /// Each bar's FFT bins and its tilt in dB.
    bands: Vec<(usize, usize, f32)>,
    /// The loudest recent moment, in dB: rises at once, sinks slowly.
    ceiling: f32,
}

// Tuned on real songs: bars average about half height, and their overall
// height follows the music's loudness closely.
/// Bars show this many dB below the ceiling.
const RANGE_DB: f32 = 42.0;
/// dB per octave added above 1 kHz (and taken off below).
const TILT_DB: f32 = 3.0;
/// How fast the ceiling sinks: 2.4 dB per second at 30 frames a second.
const SINK_DB: f32 = 0.08;
/// > 1 spreads loud and quiet further apart.
const CONTRAST: f32 = 1.6;
/// The ceiling follows the average of this many loudest bars, not the single
/// loudest (one spike would push everything else down).
const TOP: usize = 12;

impl Analyzer {
    pub fn new() -> Self {
        let fft = FftPlanner::<f32>::new().plan_fft_forward(FFT_SIZE);
        let scratch = vec![Complex::default(); fft.get_inplace_scratch_len()];
        let hann = (0..FFT_SIZE)
            .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / (FFT_SIZE - 1) as f32).cos()))
            .collect();
        let mut a = Analyzer {
            fft,
            scratch,
            hann,
            bands: Vec::new(),
            ceiling: SILENCE_DB,
        };
        a.set_bands();
        a
    }

    fn set_bands(&mut self) {
        let (log_min, log_max) = (20.0f32.ln(), 18000.0f32.ln());
        let edge = |i: usize| (log_min + i as f32 * (log_max - log_min) / NUM_BARS as f32).exp();
        let bin = |f: f32| f * FFT_SIZE as f32 / SAMPLE_RATE;
        self.bands = (0..NUM_BARS)
            .map(|i| {
                let (lo, hi) = (edge(i), edge(i + 1));
                let first = (bin(lo) as usize).max(1);
                let end = ((bin(hi) as usize) + 1).min(FFT_SIZE / 2).max(first + 1);
                let centre = (lo * hi).sqrt();
                (first, end, TILT_DB * (centre / 1000.0).log2())
            })
            .collect();
    }

    pub fn frame(&mut self, samples: &[f32]) -> Vec<f32> {
        let mut buf: Vec<Complex<f32>> = samples
            .iter()
            .zip(&self.hann)
            .map(|(&s, &w)| Complex { re: s * w, im: 0.0 })
            .collect();
        self.fft.process_with_scratch(&mut buf, &mut self.scratch);
        let scale = FFT_SIZE as f32 / 4.0;

        let db: Vec<f32> = self
            .bands
            .iter()
            .map(|&(first, end, tilt)| {
                let m = buf[first..end].iter().map(|c| c.norm()).fold(0.0f32, f32::max) / scale;
                20.0 * m.max(1e-9).log10() + tilt
            })
            .collect();

        let mut sorted = db.clone();
        sorted.sort_by(|a, b| b.total_cmp(a));
        let loudest = sorted[..TOP].iter().sum::<f32>() / TOP as f32;
        self.ceiling = loudest.max(self.ceiling - SINK_DB).max(SILENCE_DB);
        let floor = self.ceiling - RANGE_DB;
        db.iter()
            .map(|&d| ((d - floor) / RANGE_DB).clamp(0.0, 1.0).powf(CONTRAST))
            .collect()
    }
}

/// Below this the ceiling doesn't sink, so silence stays flat instead of
/// the scale chasing the noise floor up.
const SILENCE_DB: f32 = -50.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_is_flat_and_a_tone_lights_its_own_bar() {
        let mut a = Analyzer::new();
        assert!(a.frame(&vec![0.0; FFT_SIZE]).iter().all(|&b| b == 0.0));

        // A 1 kHz tone at a moderate level.
        let tone: Vec<f32> = (0..FFT_SIZE).map(|i| 0.3 * (2.0 * PI * 1000.0 * i as f32 / SAMPLE_RATE).sin()).collect();
        let bars = a.frame(&tone);
        let bin = (1000.0 * FFT_SIZE as f32 / SAMPLE_RATE).round() as usize;
        let own = a.bands.iter().position(|&(first, end, _)| first <= bin && bin < end).unwrap();
        assert!(bars[own] > 0.9, "the tone's bar reaches the top");
        // Bars far from 1 kHz (below ~100 Hz, above ~8 kHz) stay low.
        assert!(bars[..own / 2].iter().all(|&b| b < 0.2));
        assert!(bars[NUM_BARS - 20..].iter().all(|&b| b < 0.2));
    }
}
