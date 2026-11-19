use crate::config::*;
use std::{
    cell::{Cell, RefCell},
    ptr::null,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
};
thread_local! {
 static TIP:RefCell<String>=RefCell::new("Подключение к устройству вывода".into());
 static CFG:RefCell<Config>=RefCell::new(Config::load());
 static EDIT:Cell<bool>=Cell::new(false);
 static MANUALLY_HIDDEN:Cell<bool>=Cell::new(false);
 static LAST_SOUND:Cell<Instant>=Cell::new(Instant::now());
 static PANEL:Cell<HWND>=Cell::new(0);
 static TASKBAR:Cell<u32>=Cell::new(0);
 static OUTPUT:Cell<HWND>=Cell::new(0);
 static IDS:RefCell<Vec<String>>=RefCell::new(Vec::new());
}
pub const CLASS: &str = "SpectrumOverlaySettings2";
const ICONMSG: u32 = WM_APP + 1;
unsafe fn warn(h: HWND, s: &str) {
    MessageBoxW(
        h,
        wide(s).as_ptr(),
        wide("Spectrum Overlay").as_ptr(),
        MB_OK | MB_ICONWARNING,
    );
}
unsafe fn save(h: HWND, c: Config) -> bool {
    if let Err(e) = c.save() {
        warn(h, &format!("Ошибка сохранения: {e}"));
        return false;
    }
    crate::audio::DETAIL.store(c.detail, std::sync::atomic::Ordering::Relaxed);
    CFG.replace(c);
    true
}
pub unsafe fn place(h: HWND) {
    if EDIT.get() {
        return;
    }
    let monitors = screens();
    let mut c = CFG.with_borrow(Clone::clone);
    let Some(m) = monitors.get(c.selected(&monitors)) else {
        return;
    };
    if c.monitor.is_empty() && !m.primary {
        c.monitor = m.id.clone();
        save(h, c.clone());
    }
    let (x, y, w, height) = c.geometry(m);
    SetWindowPos(h, HWND_TOPMOST, x, y, w, height, SWP_NOACTIVATE);
}
pub fn appearance() -> (bool, u8, bool) {
    CFG.with_borrow(|c| (c.fill, (c.alpha * 255 / 100) as u8, EDIT.get()))
}
pub unsafe fn app_icon(size: i32) -> HICON {
    LoadImageW(
        GetModuleHandleW(null()),
        101 as *const u16,
        IMAGE_ICON,
        size,
        size,
        LR_SHARED,
    )
}
unsafe fn tray(h: HWND, op: u32) {
    let mut n: NOTIFYICONDATAW = std::mem::zeroed();
    n.cbSize = std::mem::size_of_val(&n) as u32;
    n.hWnd = h;
    n.uID = 1;
    n.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    n.uCallbackMessage = ICONMSG;
    n.hIcon = app_icon(GetSystemMetrics(SM_CXSMICON));
    TIP.with_borrow(|status| {
        let tip = format!("Spectrum Overlay — {status}");
        let mut index = 0;
        for ch in tip.chars() {
            if index + ch.len_utf16() >= n.szTip.len() {
                break;
            }
            index += ch.encode_utf16(&mut n.szTip[index..]).len();
        }
    });
    Shell_NotifyIconW(op, &n);
}
pub unsafe fn set_status(h: HWND, status: String) {
    if TIP.with_borrow(|old| old == &status) {
        return;
    }
    TIP.replace(status);
    tray(h, NIM_MODIFY);
}
pub unsafe fn init(h: HWND) {
    crate::audio::DETAIL.store(
        CFG.with_borrow(|c| c.detail),
        std::sync::atomic::Ordering::Relaxed,
    );
    TASKBAR.set(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()));
    OUTPUT.set(h);
    place(h);
    tray(h, NIM_ADD);
}
unsafe fn edit(h: HWND, on: bool, commit: bool) {
    if !on && commit {
        let mut r: RECT = std::mem::zeroed();
        GetWindowRect(h, &mut r);
        let mid = MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of_val(&info) as u32;
        GetMonitorInfoW(mid, &mut info);
        if let Some(m) = screens()
            .into_iter()
            .find(|m| m.rect.left == info.rcWork.left && m.rect.top == info.rcWork.top)
        {
            let dip = |v: i32| (v * 96 + m.dpi / 2) / m.dpi;
            let mut c = CFG.with_borrow(Clone::clone);
            c.monitor = m.id;
            c.x = dip(r.left - m.rect.left).max(0);
            c.y = dip(r.top - m.rect.top).max(0);
            c.w = dip(r.right - r.left).max(120);
            c.h = dip(r.bottom - r.top).max(50);
            if !save(h, c) {
                return;
            }
        }
    }
    EDIT.set(on);
    LAST_SOUND.set(Instant::now());
    if on {
        MANUALLY_HIDDEN.set(false);
    }
    let base = WS_EX_TOPMOST | WS_EX_TOOLWINDOW;
    SetWindowLongW(
        h,
        GWL_EXSTYLE,
        if on {
            base
        } else {
            base | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE
        } as i32,
    );
    SetWindowLongW(
        h,
        GWL_STYLE,
        (WS_VISIBLE | WS_POPUP | if on { WS_THICKFRAME | WS_CAPTION } else { 0 }) as i32,
    );
    SetWindowPos(
        h,
        HWND_TOPMOST,
        0,
        0,
        0,
        0,
        SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW | SWP_NOACTIVATE,
    );
    if on {
        SetWindowTextW(
            h,
            wide("Сохранение и отмена — в меню значка в трее").as_ptr(),
        );
        SetForegroundWindow(h);
        SetFocus(h);
    } else {
        place(h);
    }
}
unsafe fn toggle(h: HWND) {
    if EDIT.get() {
        edit(h, false, false);
    }
    MANUALLY_HIDDEN.set(IsWindowVisible(h) != 0);
    LAST_SOUND.set(Instant::now());
    update_visibility(h);
}

fn should_show(manual_hide: bool, editing: bool, quiet: Duration, delay: i32) -> bool {
    editing || (!manual_hide && (delay == 0 || quiet < Duration::from_secs(delay as u64)))
}

pub unsafe fn update_visibility(h: HWND) -> bool {
    if crate::audio::SOUND_DETECTED.swap(false, std::sync::atomic::Ordering::Relaxed) {
        LAST_SOUND.set(Instant::now());
    }
    let show = should_show(
        MANUALLY_HIDDEN.get(),
        EDIT.get(),
        LAST_SOUND.get().elapsed(),
        CFG.with_borrow(|c| c.silence_seconds),
    );
    if show != (IsWindowVisible(h) != 0) {
        ShowWindow(h, if show { SW_SHOWNOACTIVATE } else { SW_HIDE });
    }
    show
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn silence_and_manual_visibility() {
        let timeout = Duration::from_secs(3);
        assert!(should_show(false, false, Duration::ZERO, 3));
        assert!(!should_show(false, false, timeout, 3));
        assert!(should_show(false, false, timeout, 0));
        assert!(!should_show(true, false, Duration::ZERO, 0));
        assert!(should_show(false, true, timeout, 3));
    }
}
unsafe fn menu(h: HWND) {
    let m = CreatePopupMenu();
    for (id, label) in [
        (1, "Настройки…"),
        (2, "Изменить положение и размер"),
        (3, "Показать / скрыть"),
        (4, "Выход"),
    ] {
        AppendMenuW(m, MF_STRING, id, wide(label).as_ptr());
    }
    if EDIT.get() {
        AppendMenuW(m, MF_STRING, 5, wide("Сохранить положение").as_ptr());
        AppendMenuW(m, MF_STRING, 6, wide("Отменить перемещение").as_ptr());
    }
    let mut p: POINT = std::mem::zeroed();
    GetCursorPos(&mut p);
    SetForegroundWindow(h);
    let id = TrackPopupMenu(m, TPM_RETURNCMD | TPM_RIGHTBUTTON, p.x, p.y, 0, h, null());
    DestroyMenu(m);
    match id {
        5 => edit(h, false, true),
        6 => edit(h, false, false),
        1 => panel(),
        2 => edit(h, true, false),
        3 => toggle(h),
        4 => {
            DestroyWindow(h);
        }
        _ => (),
    }
}
pub unsafe extern "system" fn overlay(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST if !EDIT.get() => HTTRANSPARENT as isize,
        WM_NCHITTEST if EDIT.get() => {
            let result = DefWindowProcW(h, msg, w, l);
            if result == HTCLIENT as isize {
                HTCAPTION as isize
            } else {
                result
            }
        }
        msg if msg != 0 && msg == TASKBAR.get() => {
            tray(h, NIM_ADD);
            0
        }
        ICONMSG => {
            match l as u32 {
                WM_RBUTTONUP => menu(h),
                WM_LBUTTONUP => panel(),
                _ => (),
            }
            0
        }
        WM_DISPLAYCHANGE | WM_SETTINGCHANGE => {
            if OUTPUT.get() == h {
                place(h);
            }
            0
        }
        WM_DESTROY => {
            tray(h, NIM_DELETE);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(h, msg, w, l),
    }
}
unsafe fn control(
    h: HWND,
    class: &str,
    label: &str,
    id: isize,
    x: i32,
    y: i32,
    w: i32,
    height: i32,
    style: u32,
) -> HWND {
    CreateWindowExW(
        0,
        wide(class).as_ptr(),
        wide(label).as_ptr(),
        WS_VISIBLE | WS_CHILD | style,
        x,
        y,
        w,
        height,
        h,
        id,
        GetModuleHandleW(null()),
        null(),
    )
}
unsafe fn panel() {
    let h = PANEL.get();
    if h != 0 {
        SetForegroundWindow(h);
        return;
    }
    if EDIT.get() {
        edit(OUTPUT.get(), false, false);
    }
    let h = CreateWindowExW(
        WS_EX_TOPMOST,
        wide(CLASS).as_ptr(),
        wide("Настройки спектра").as_ptr(),
        WS_CAPTION | WS_SYSMENU,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        435,
        478,
        0,
        0,
        GetModuleHandleW(null()),
        null(),
    );
    if h == 0 {
        return;
    }
    PANEL.set(h);
    let c = CFG.with_borrow(Clone::clone);
    let monitors = screens();
    let selected = c.selected(&monitors);
    control(h, "STATIC", "Экран", 0, 16, 20, 145, 22, 0);
    let combo = control(
        h,
        "COMBOBOX",
        "",
        10,
        166,
        16,
        238,
        210,
        CBS_DROPDOWNLIST as u32 | WS_VSCROLL,
    );
    for m in &monitors {
        SendMessageW(
            combo,
            CB_ADDSTRING,
            0,
            wide(&format!(
                "{}{}",
                m.id.trim_start_matches(r"\\.\"),
                if m.primary { " (основной)" } else { "" }
            ))
            .as_ptr() as isize,
        );
    }
    IDS.replace(monitors.into_iter().map(|m| m.id).collect());
    SendMessageW(combo, CB_SETCURSEL, selected, 0);
    for (idx, (label, n)) in [
        ("Ширина (DIP)", c.w),
        ("Высота (DIP)", c.h),
        ("Видимость (%)", c.alpha),
    ]
    .into_iter()
    .enumerate()
    {
        let y = 58 + idx as i32 * 34;
        control(h, "STATIC", label, 0, 16, y, 145, 23, 0);
        control(
            h,
            "EDIT",
            &n.to_string(),
            11 + idx as isize,
            166,
            y,
            130,
            24,
            WS_BORDER | ES_NUMBER as u32,
        );
    }
    control(
        h,
        "STATIC",
        "Скрыть после тишины, с (0 — выкл.)",
        0,
        16,
        163,
        290,
        24,
        0,
    );
    control(
        h,
        "EDIT",
        &c.silence_seconds.to_string(),
        18,
        310,
        160,
        88,
        24,
        WS_BORDER | ES_NUMBER as u32,
    );
    control(h, "STATIC", "Детализация", 0, 16, 202, 145, 24, 0);
    let quality = control(
        h,
        "COMBOBOX",
        "",
        19,
        166,
        198,
        238,
        120,
        CBS_DROPDOWNLIST as u32,
    );
    for label in [
        "Низкая — 64 точки",
        "Средняя — 192 точки",
        "Высокая — 512 точек",
    ] {
        SendMessageW(quality, CB_ADDSTRING, 0, wide(label).as_ptr() as isize);
    }
    SendMessageW(quality, CB_SETCURSEL, c.detail, 0);
    let cb = control(
        h,
        "BUTTON",
        "Запускать при входе в Windows",
        14,
        16,
        244,
        380,
        25,
        BS_AUTOCHECKBOX as u32,
    );
    SendMessageW(cb, BM_SETCHECK, auto_enabled() as usize, 0);
    let fill = control(
        h,
        "BUTTON",
        "Заливка под спектром",
        20,
        16,
        275,
        380,
        25,
        BS_AUTOCHECKBOX as u32,
    );
    SendMessageW(fill, BM_SETCHECK, c.fill as usize, 0);
    control(h, "BUTTON", "Вернуть в угол", 15, 16, 317, 150, 30, 0);
    control(h, "BUTTON", "Переместить мышью", 16, 175, 317, 224, 30, 0);
    control(h, "BUTTON", "Применить", 17, 255, 370, 144, 32, 0);
    ShowWindow(h, SW_SHOW);
    SetForegroundWindow(h);
}
unsafe fn apply(h: HWND, id: u16) -> bool {
    let mut c = CFG.with_borrow(Clone::clone);
    for (i, dst, lo, hi) in [
        (11, &mut c.w, 120, 16384),
        (12, &mut c.h, 50, 16384),
        (13, &mut c.alpha, 10, 100),
        (18, &mut c.silence_seconds, 0, 300),
    ] {
        let mut buf = [0u16; 32];
        let n = GetDlgItemTextW(h, i, buf.as_mut_ptr(), 32);
        let value = String::from_utf16_lossy(&buf[..n as usize])
            .parse::<i32>()
            .ok();
        match value {
            Some(v) if (lo..=hi).contains(&v) => *dst = v,
            _ => {
                warn(h, &format!("Допустимый диапазон поля: {lo}–{hi}"));
                return false;
            }
        }
    }
    let index = SendDlgItemMessageW(h, 10, CB_GETCURSEL, 0, 0) as usize;
    if let Some(m) = IDS.with_borrow(|v| v.get(index).cloned()) {
        if c.monitor != m || id == 15 {
            c.x = -1;
            c.y = -1;
        }
        c.monitor = m;
    }
    let detail = SendDlgItemMessageW(h, 19, CB_GETCURSEL, 0, 0) as usize;
    c.fill = SendDlgItemMessageW(h, 20, BM_GETCHECK, 0, 0) != 0;
    if detail <= crate::spectrum::HIGH_DETAIL {
        c.detail = detail;
    }
    let on = SendDlgItemMessageW(h, 14, BM_GETCHECK, 0, 0) != 0;
    if on != auto_enabled() {
        if let Err(e) = autorun(on) {
            warn(h, &e.to_string());
            return false;
        }
    }
    if !save(h, c) {
        return false;
    }
    place(OUTPUT.get());
    true
}
pub unsafe extern "system" fn settings(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND if matches!(w as u16, 15..=17) => {
            let id = w as u16;
            if apply(h, id) {
                DestroyWindow(h);
                if id == 16 {
                    edit(OUTPUT.get(), true, false);
                }
            }
            0
        }
        WM_DESTROY => {
            PANEL.set(0);
            0
        }
        _ => DefWindowProcW(h, msg, w, l),
    }
}
