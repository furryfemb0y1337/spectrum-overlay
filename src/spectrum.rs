use rustfft::{num_complex::Complex32, Fft, FftPlanner};
use std::sync::Arc;

pub const POINTS: usize = 512;
pub const DETAIL_POINTS: [usize; 3] = [64, 192, POINTS];
pub const HIGH_DETAIL: usize = DETAIL_POINTS.len() - 1;
pub const HISTORY: usize = 65536;

struct Band {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    bins: Vec<Complex32>,
    scratch: Vec<Complex32>,
    power: Vec<f32>,
}
impl Band {
    fn new(n: usize, planner: &mut FftPlanner<f32>) -> Self {
        let fft = planner.plan_fft_forward(n);
        Self {
            scratch: vec![Complex32::default(); fft.get_inplace_scratch_len()],
            fft,
            window: (0..n)
                .map(|i| (1.0 - (std::f32::consts::TAU * i as f32 / n as f32).cos()) * 0.5)
                .collect(),
            bins: vec![Complex32::default(); n],
            power: vec![0.; n / 2 + 1],
        }
    }
    fn analyze(&mut self, history: &[f32], end: usize, channels: usize) {
        let n = self.bins.len();
        self.power.fill(0.);
        for channel in 0..channels {
            for i in 0..n {
                self.bins[i] = Complex32::new(
                    history[((end + HISTORY - n + i) % HISTORY) * channels + channel]
                        * self.window[i],
                    0.,
                );
            }
            self.fft
                .process_with_scratch(&mut self.bins, &mut self.scratch);
            for (i, p) in self.power.iter_mut().enumerate() {
                let gain = if i == 0 || i == n / 2 { 2. } else { 4. };
                *p += self.bins[i].norm_sqr() * (gain / n as f32).powi(2) / channels as f32;
            }
        }
    }
    fn peak(&self, lo: f32, hi: f32, rate: f32) -> f32 {
        let scale = self.bins.len() as f32 / rate;
        let last = self.power.len() - 1;
        let a = ((lo * scale).ceil() as usize).min(last);
        let b = ((hi * scale).floor() as usize).min(last);
        if a > b {
            let x = (lo + hi) * 0.5 * scale;
            let i = (x as usize).min(last);
            return self.power[i] * (1. - x.fract()) + self.power[(i + 1).min(last)] * x.fract();
        }
        self.power[a..=b].iter().copied().fold(0., f32::max)
    }
}
pub fn frequency(x: f32, rate: f32) -> f32 {
    if x <= 0.045 {
        20. * x / 0.045
    } else {
        20. * (rate / 40.).powf((x - 0.045) / 0.955)
    }
}
pub struct Analyzer {
    pub detail: usize,
    bands: [Band; 3],
}
impl Analyzer {
    pub fn new(detail: usize) -> Self {
        let mut planner = FftPlanner::new();
        let divisor = 1 << (HIGH_DETAIL - detail);
        Self {
            detail,
            bands: [65536, 16384, 4096].map(|n| Band::new(n / divisor, &mut planner)),
        }
    }
    pub fn frame(&mut self, h: &[f32], end: usize, channels: usize, rate: f32) -> [f32; POINTS] {
        for b in &mut self.bands {
            b.analyze(h, end, channels);
        }
        std::array::from_fn(|i| {
            let lo = frequency(i as f32 / POINTS as f32, rate);
            let hi = frequency((i + 1) as f32 / POINTS as f32, rate);
            let f = (lo + hi) * 0.5;
            let (a, b, w) = if f < 320. {
                (0, 1, ((f - 200.) / 120.).clamp(0., 1.))
            } else {
                (1, 2, ((f - 3200.) / 1600.).clamp(0., 1.))
            };
            let p =
                self.bands[a].peak(lo, hi, rate) * (1. - w) + self.bands[b].peak(lo, hi, rate) * w;
            ((10. * p.max(1e-10).log10() + 80.) / 80.).clamp(0., 1.)
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detail_presets_keep_tone_on_frequency_axis() {
        let samples: Vec<_> = (0..HISTORY)
            .map(|i| (std::f32::consts::TAU * 1500. * i as f32 / 48000.).sin())
            .collect();
        for (detail, columns) in DETAIL_POINTS.into_iter().enumerate() {
            let mut analyzer = Analyzer::new(detail);
            let frame = analyzer.frame(&samples, 123, 1, 48000.);
            let peak = (0..columns)
                .max_by(|a, b| {
                    let level = |i: usize| {
                        frame[i * POINTS / columns..(i + 1) * POINTS / columns]
                            .iter()
                            .copied()
                            .fold(0., f32::max)
                    };
                    level(*a).total_cmp(&level(*b))
                })
                .unwrap();
            let center = frequency((peak as f32 + 0.5) / columns as f32, 48000.);
            assert!(
                (center - 1500.).abs() < 110.,
                "detail {detail}, peak {center}"
            );
        }
    }
    #[test]
    fn surround_center_tone() {
        let mut analyzer = Analyzer::new(HIGH_DETAIL);
        let mut samples = vec![0.; HISTORY * 6];
        for (i, frame) in samples.chunks_exact_mut(6).enumerate() {
            frame[2] = (std::f32::consts::TAU * 750. * i as f32 / 48000.).sin();
        }
        let spectrum = analyzer.frame(&samples, 0, 6, 48000.);
        let (bin, level) = spectrum
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap();
        assert!((frequency((bin as f32 + 0.5) / POINTS as f32, 48000.) - 750.).abs() < 15.);
        assert!((*level - 0.903).abs() < 0.02);
    }
    #[test]
    fn full_band_antiphase() {
        let mut analyzer = Analyzer::new(HIGH_DETAIL);
        for f in [30., 1000., 20000., 30000., 47000.] {
            let h = (0..HISTORY)
                .flat_map(|i| {
                    let v = (std::f32::consts::TAU * f * i as f32 / 96000.).sin();
                    [v, -v]
                })
                .collect::<Vec<_>>();
            let frame = analyzer.frame(&h, 0, 2, 96000.);
            let (i, v) = frame
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .unwrap();
            assert!(
                *v > 0.92
                    && (frequency((i as f32 + 0.5) / POINTS as f32, 96000.) - f).abs()
                        < 3. + f * 0.025
            );
        }
    }
}
