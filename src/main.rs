#![windows_subsystem = "windows"]
mod audio;
mod config;
mod spectrum;
mod ui;
use spectrum::POINTS;
use std::{
    ptr::{null, null_mut},
    sync::{Arc, Mutex},
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::*, Graphics::Gdi::*, System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::*,
};

unsafe fn run() {
    use windows_sys::Win32::{System::Threading::CreateMutexW, UI::HiDpi::*};
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    let singleton = CreateMutexW(
        null(),
        0,
        config::wide("Local\\SpectrumOverlay.v2").as_ptr(),
    );
    if singleton == 0 || GetLastError() == ERROR_ALREADY_EXISTS {
        return;
    }
    let class: Vec<u16> = "SpectrumOverlay96\0".encode_utf16().collect();
    let instance = GetModuleHandleW(null());
    let mut wc: WNDCLASSW = std::mem::zeroed();
    wc.lpfnWndProc = Some(ui::overlay);
    wc.hInstance = instance;
    wc.hIcon = ui::app_icon(GetSystemMetrics(SM_CXICON));
    wc.lpszClassName = class.as_ptr();
    RegisterClassW(&wc);
    let panel_class = config::wide(ui::CLASS);
    wc.hbrBackground = (COLOR_BTNFACE + 1) as isize;
    wc.hCursor = LoadCursorW(0, IDC_ARROW);
    wc.lpfnWndProc = Some(ui::settings);
    wc.lpszClassName = panel_class.as_ptr();
    RegisterClassW(&wc);
    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
        class.as_ptr(),
        class.as_ptr(),
        WS_POPUP,
        0,
        0,
        480,
        140,
        0,
        0,
        instance,
        null(),
    );
    if hwnd == 0 {
        return;
    }
    ui::init(hwnd);
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    let display = Arc::new(Mutex::new((96000, [0.; POINTS])));
    let worker = display.clone();
    std::thread::spawn(move || audio::run(worker));
    let mut levels = [0f32; POINTS];
    let mut tops = Vec::<i32>::new();
    let mut msg: MSG = std::mem::zeroed();
    let mut last = Instant::now();

    SetTimer(hwnd, 1, 16, None);
    while GetMessageW(&mut msg, 0, 0, 0) > 0 {
        if msg.message != WM_TIMER {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
            continue;
        }
        let dt = last.elapsed().as_secs_f32();
        last = Instant::now();
        if let Some(status) = audio::STATUS_CHANGE.lock().unwrap().take() {
            ui::set_status(hwnd, status);
        }
        let visible = ui::update_visibility(hwnd);
        let was_visible = audio::VISIBLE.swap(visible, std::sync::atomic::Ordering::Relaxed);
        if !visible {
            if was_visible {
                levels.fill(0.);
                display.lock().unwrap().1.fill(0.);
            }
            continue;
        }
        let (rate, target) = *display.lock().unwrap();
        let mut client: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut client);
        let width = client.right;
        let height = client.bottom;
        if width < 2 || height < 22 {
            continue;
        }
        let dc = GetDC(hwnd);
        let back = CreateCompatibleDC(dc);
        let (fill, opacity, editing) = ui::appearance();
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        let mut bits = null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, 0, 0);
        if bitmap == 0 {
            DeleteDC(back);
            ReleaseDC(hwnd, dc);
            continue;
        }
        let old = SelectObject(back, bitmap);
        PatBlt(back, 0, 0, width, height, BLACKNESS);
        let pen = SelectObject(back, GetStockObject(DC_PEN));
        SetDCPenColor(back, 0x00e8dcc2);
        for (i, level) in levels.iter_mut().enumerate() {
            *level = target[i].max((*level - dt * 1.7).max(0.));
        }
        let detail = audio::DETAIL.load(std::sync::atomic::Ordering::Relaxed);
        let columns = (width as usize).min(spectrum::DETAIL_POINTS[detail]);
        let mut previous_peak = false;
        let mut previous = (0, 0);
        if fill {
            tops.resize(width as usize, height - 18);
        }
        for i in 0..columns {
            let peak = target[i * POINTS / columns..(i + 1) * POINTS / columns]
                .iter()
                .any(|v| *v >= 1.0 - f32::EPSILON);
            SetDCPenColor(
                back,
                if peak || previous_peak {
                    0x004040ff
                } else {
                    0x00e8dcc2
                },
            );
            previous_peak = peak;
            let level = levels[i * POINTS / columns..(i + 1) * POINTS / columns]
                .iter()
                .copied()
                .fold(0., f32::max);
            let x = i as i32 * (width - 1) / (columns as i32 - 1);
            let y = height - 18 - (level * (height - 20) as f32) as i32;
            if i == 0 {
                MoveToEx(back, x, y, null_mut());
            } else {
                LineTo(back, x, y);
                if fill {
                    let (px, py) = previous;
                    for column in px..=x {
                        tops[column as usize] = py + (y - py) * (column - px) / (x - px);
                    }
                }
            }
            previous = (x, y);
        }
        SetBkMode(back, TRANSPARENT as i32);
        SetTextColor(back, 0x00b4afa5);
        let mut label_end = -1;
        for (f, label) in [
            (100., "100"),
            (1000., "1k"),
            (10000., "10k"),
            (20000., "20k"),
            (rate as f32 / 2., ""),
        ] {
            if f > rate as f32 / 2. {
                continue;
            }
            let s = if label.is_empty() {
                format!("{:.1}k", f / 1000.)
            } else {
                label.into()
            };
            let t: Vec<u16> = s.encode_utf16().collect();
            let x =
                (width as f32 * (0.045 + 0.955 * (f / 20.).ln() / (rate as f32 / 40.).ln())) as i32;
            if !label.is_empty() && (x < label_end || x + 28 > width - 48) {
                continue;
            }
            label_end = x + 30;
            TextOutW(
                back,
                x.min(width - 48),
                height - 17,
                t.as_ptr(),
                t.len() as i32,
            );
        }
        GdiFlush();
        let pixels =
            std::slice::from_raw_parts_mut(bits.cast::<u32>(), width as usize * height as usize);
        for (y, row) in pixels.chunks_exact_mut(width as usize).enumerate() {
            for (x, pixel) in row.iter_mut().enumerate() {
                if *pixel & 0x00ff_ffff != 0 {
                    *pixel |= 0xff00_0000;
                } else if fill {
                    *pixel = fill_pixel(y as i32, tops[x], height - 18);
                }
            }
        }
        if editing {
            BitBlt(dc, 0, 0, width, height, back, 0, 0, SRCCOPY);
        } else {
            let size = SIZE {
                cx: width,
                cy: height,
            };
            let origin = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: opacity,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            UpdateLayeredWindow(hwnd, 0, null(), &size, back, &origin, 0, &blend, ULW_ALPHA);
        }
        SelectObject(back, pen);
        SelectObject(back, old);
        DeleteObject(bitmap);
        DeleteDC(back);
        ReleaseDC(hwnd, dc);
    }
}
fn fill_pixel(y: i32, top: i32, bottom: i32) -> u32 {
    if y < top || y >= bottom {
        return 0;
    }
    let alpha = (112 * (bottom - y) / (bottom - top)) as u32;
    (alpha << 24) | (194 * alpha / 255 << 16) | (220 * alpha / 255 << 8) | (232 * alpha / 255)
}

#[test]
fn fill_fades_to_transparency_within_spectrum() {
    assert_eq!(fill_pixel(3, 4, 12), 0);
    assert_eq!(fill_pixel(12, 12, 12), 0);
    assert_eq!(fill_pixel(12, 4, 12), 0);
    assert_eq!(fill_pixel(4, 4, 12) >> 24, 112);
    assert_eq!(fill_pixel(8, 4, 12) >> 24, 56);
}

fn main() {
    unsafe { run() }
}
