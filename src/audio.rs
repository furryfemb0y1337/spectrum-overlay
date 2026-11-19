use crate::spectrum::{history_frames, Analyzer, POINTS};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, FromSample, SampleFormat, SizedSample, Stream, StreamConfig,
};
use ringbuf::{HeapProducer, HeapRb};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
pub static VISIBLE: AtomicBool = AtomicBool::new(true);
pub static DETAIL: AtomicUsize = AtomicUsize::new(crate::spectrum::HIGH_DETAIL);
pub static SOUND_DETECTED: AtomicBool = AtomicBool::new(false);
pub static STATUS_CHANGE: Mutex<Option<String>> = Mutex::new(None);
pub type Display = Arc<Mutex<(u32, [f32; POINTS])>>;
fn capture<T: SizedSample + Copy>(
    device: &Device,
    config: &StreamConfig,
    mut queue: HeapProducer<f32>,
    failed: Arc<AtomicBool>,
) -> Result<Stream, cpal::BuildStreamError>
where
    f32: FromSample<T>,
{
    let channels = config.channels as usize;
    device.build_input_stream(
        config,
        move |data: &[T], _| {
            let frames = (data.len() / channels).min(queue.free_len() / channels);
            queue.push_iter(&mut data[..frames * channels].iter().map(|v| (*v).to_sample()));
        },
        move |_| failed.store(true, Ordering::Relaxed),
        None,
    )
}
fn session(display: &Display) -> Result<(), Box<dyn std::error::Error>> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("Нет устройства вывода")?;
    let name = device.name()?;
    let format = device.default_output_config()?;
    let config = format.config();
    let rate = config.sample_rate.0;
    let channels = config.channels as usize;
    let (tx, mut rx) = HeapRb::new(rate as usize * channels).split();
    let failed = Arc::new(AtomicBool::new(false));
    macro_rules! open {
        ($t:ty) => {
            capture::<$t>(&device, &config, tx, failed.clone())?
        };
    }
    let stream = match format.sample_format() {
        SampleFormat::F32 => open!(f32),
        SampleFormat::I16 => open!(i16),
        SampleFormat::I32 => open!(i32),
        SampleFormat::U16 => open!(u16),
        SampleFormat::F64 => open!(f64),
        _ => return Err("Неподдерживаемый формат звука".into()),
    };
    stream.play()?;
    *STATUS_CHANGE.lock().unwrap() = Some(format!("Захват — {name}, {rate} Гц"));
    let mut analyzer = Analyzer::new(DETAIL.load(Ordering::Relaxed), rate);
    let mut history = vec![0.; history_frames(rate) * channels];
    *display.lock().unwrap() = (rate, [0.; POINTS]);
    let mut silent = true;
    let mut end = 0;
    let mut check = Instant::now();
    let mut last = Instant::now();
    loop {
        thread::sleep(Duration::from_millis(16));
        if failed.load(Ordering::Relaxed) {
            return Err("Поток захвата прерван".into());
        }
        if check.elapsed() > Duration::from_secs(1) {
            let current = host
                .default_output_device()
                .ok_or("Нет устройства вывода")?;
            if current.name()? != name || current.default_output_config()?.config() != config {
                return Err("Смена устройства вывода".into());
            }
            check = Instant::now();
        }
        let count = rx.len() / channels * channels;
        let mut audible = false;
        for _ in 0..count {
            if let Some(sample) = rx.pop() {
                audible |= sample.abs() > 0.0001;
                history[end] = sample;
                end = (end + 1) % history.len();
            }
        }
        if audible {
            SOUND_DETECTED.store(true, Ordering::Relaxed);
        }
        if count > 0 {
            last = Instant::now();
            silent = false;
            if VISIBLE.load(Ordering::Relaxed) {
                let detail = DETAIL.load(Ordering::Relaxed);
                if analyzer.detail != detail {
                    analyzer = Analyzer::new(detail, rate);
                }
                let frame = analyzer.frame(&history, end / channels, channels, rate as f32);
                *display.lock().unwrap() = (rate, frame);
            }
        } else if !silent && last.elapsed() > Duration::from_millis(100) {
            silent = true;
            history.fill(0.);
            *display.lock().unwrap() = (rate, [0.; POINTS]);
        }
    }
}
pub fn run(display: Display) {
    loop {
        if let Err(e) = session(&display) {
            *STATUS_CHANGE.lock().unwrap() = Some(format!("Захват недоступен — {e}"));
        }
        display.lock().unwrap().1.fill(0.);
        thread::sleep(Duration::from_secs(1));
    }
}
