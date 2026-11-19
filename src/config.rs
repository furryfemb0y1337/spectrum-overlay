use std::{fs, io, path::PathBuf, ptr::null};
use windows_sys::Win32::{Foundation::*, Graphics::Gdi::*, UI::HiDpi::*};
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
pub struct Monitor {
    pub id: String,
    pub rect: RECT,
    pub dpi: i32,
    pub primary: bool,
}
pub unsafe fn screens() -> Vec<Monitor> {
    unsafe extern "system" fn collect(m: HMONITOR, _: HDC, _: *mut RECT, p: LPARAM) -> BOOL {
        let mut info: MONITORINFOEXW = std::mem::zeroed();
        info.monitorInfo.cbSize = std::mem::size_of_val(&info) as u32;
        if GetMonitorInfoW(m, &mut info.monitorInfo) != 0 {
            let mut x = 96;
            let mut y = 96;
            GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut x, &mut y);
            let id = String::from_utf16_lossy(
                &info.szDevice[..info.szDevice.iter().position(|v| *v == 0).unwrap_or(32)],
            );
            (*(p as *mut Vec<Monitor>)).push(Monitor {
                id,
                rect: info.monitorInfo.rcWork,
                dpi: x.max(1) as i32,
                primary: info.monitorInfo.dwFlags & 1 != 0,
            });
        }
        1
    }
    let mut v = Vec::new();
    EnumDisplayMonitors(0, null(), Some(collect), &mut v as *mut _ as isize);
    v
}
#[derive(Clone)]
pub struct Config {
    pub monitor: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub alpha: i32,
    pub silence_seconds: i32,
    pub detail: usize,
    pub fill: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            monitor: String::new(),
            x: -1,
            y: -1,
            w: 480,
            h: 140,
            alpha: 100,
            silence_seconds: 3,
            detail: crate::spectrum::HIGH_DETAIL,
            fill: false,
        }
    }
}
impl Config {
    fn path() -> io::Result<PathBuf> {
        Ok(
            PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or(io::ErrorKind::NotFound)?)
                .join("SpectrumOverlay/settings.toml"),
        )
    }
    pub fn load() -> Self {
        let mut c = Self::default();
        if let Ok(s) = Self::path().and_then(fs::read_to_string) {
            for line in s.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    let v = v.trim();
                    if k.trim() == "monitor" {
                        c.monitor = v.trim_matches('\'').into();
                        continue;
                    }
                    if let Ok(n) = v.parse::<i32>() {
                        match k.trim() {
                            "x" => c.x = n,
                            "y" => c.y = n,
                            "width" => c.w = n.clamp(120, 16384),
                            "height" => c.h = n.clamp(50, 16384),
                            "opacity" => c.alpha = n.clamp(10, 100),
                            "silence_seconds" => c.silence_seconds = n.clamp(0, 300),
                            "fill" => c.fill = n == 1,
                            "detail" if (0..=crate::spectrum::HIGH_DETAIL as i32).contains(&n) => {
                                c.detail = n as usize
                            }
                            _ => (),
                        }
                    }
                }
            }
        }
        c
    }
    pub fn save(&self) -> io::Result<()> {
        let p = Self::path()?;
        fs::create_dir_all(p.parent().unwrap())?;
        let tmp = p.with_extension("tmp");
        fs::write(
            &tmp,
            format!(
                "monitor = '{}'\nx = {}\ny = {}\nwidth = {}\nheight = {}\nopacity = {}\nsilence_seconds = {}\ndetail = {}\nfill = {}\n",
                self.monitor, self.x, self.y, self.w, self.h, self.alpha, self.silence_seconds, self.detail, u8::from(self.fill)
            ),
        )?;
        fs::rename(tmp, p)
    }
    pub fn selected(&self, v: &[Monitor]) -> usize {
        v.iter()
            .position(|m| m.id == self.monitor)
            .or_else(|| {
                if self.monitor.is_empty() {
                    v.iter().position(|m| !m.primary)
                } else {
                    v.iter().position(|m| m.primary)
                }
            })
            .unwrap_or(0)
    }
    pub fn geometry(&self, m: &Monitor) -> (i32, i32, i32, i32) {
        fn axis(pos: i32, size: i32, lo: i32, hi: i32, dpi: i32) -> (i32, i32) {
            let scale =
                |n: i32| ((n as i64 * dpi as i64 + 48) / 96).clamp(0, i32::MAX as i64) as i32;
            let size = scale(size).min(hi - lo);
            let room = hi - lo - size;
            let offset = if pos < 0 {
                room - scale(16)
            } else {
                scale(pos)
            };
            (lo + offset.clamp(0, room), size)
        }
        let (x, w) = axis(self.x, self.w, m.rect.left, m.rect.right, m.dpi);
        let (y, h) = axis(self.y, self.h, m.rect.top, m.rect.bottom, m.dpi);
        (x, y, w, h)
    }
}
use windows_sys::Win32::System::Registry::*;
const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub unsafe fn auto_enabled() -> bool {
    RegGetValueW(
        HKEY_CURRENT_USER,
        wide(RUN).as_ptr(),
        wide("SpectrumOverlay").as_ptr(),
        RRF_RT_REG_SZ,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
    ) == 0
}
pub unsafe fn autorun(on: bool) -> io::Result<()> {
    let mut key = 0;
    let e = RegCreateKeyExW(
        HKEY_CURRENT_USER,
        wide(RUN).as_ptr(),
        0,
        null(),
        0,
        KEY_SET_VALUE,
        null(),
        &mut key,
        std::ptr::null_mut(),
    );
    if e != 0 {
        return Err(io::Error::from_raw_os_error(e as i32));
    }
    let name = wide("SpectrumOverlay");
    let e = if on {
        let data = wide(&format!("\"{}\"", std::env::current_exe()?.display()));
        RegSetValueExW(
            key,
            name.as_ptr(),
            0,
            REG_SZ,
            data.as_ptr() as _,
            (data.len() * 2) as u32,
        )
    } else {
        RegDeleteValueW(key, name.as_ptr())
    };
    RegCloseKey(key);
    if e == 0 || (!on && e == 2) {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(e as i32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn monitor(primary: bool) -> Monitor {
        Monitor {
            id: if primary { "main" } else { "secondary" }.into(),
            rect: RECT {
                left: -1920,
                top: 0,
                right: 0,
                bottom: 1080,
            },
            dpi: 144,
            primary,
        }
    }
    #[test]
    fn default_on_left_secondary_at_150_percent() {
        let screens = [monitor(true), monitor(false)];
        let c = Config::default();
        assert_eq!(c.selected(&screens), 1);
        assert_eq!(c.geometry(&screens[1]), (-744, 846, 720, 210));
    }
    #[test]
    fn missing_screen_falls_back_and_recovers() {
        let c = Config {
            monitor: "secondary".into(),
            x: 99999,
            y: 99999,
            ..Config::default()
        };
        assert_eq!(c.selected(&[monitor(true)]), 0);
        assert_eq!(c.selected(&[monitor(true), monitor(false)]), 1);
        assert_eq!(c.geometry(&monitor(false)), (-720, 870, 720, 210));
    }
}
