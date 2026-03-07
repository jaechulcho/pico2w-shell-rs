#![allow(dead_code)]

use core::fmt::Write as _;

use embassy_rp::Peri;
use embassy_rp::aon_timer::{AonTimer, DateTime};
use embassy_rp::flash::{Async, Flash};
use embassy_rp::peripherals::FLASH;
use embassy_rp::peripherals::POWMAN;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::Instant;
use littlefs2::driver::Storage;
use littlefs2::fs::{Allocation, Filesystem};
use static_cell::StaticCell;

pub const FLASH_SIZE: usize = 4 * 1024 * 1024;
pub const LOG_OFFSET: u32 = 2 * 1024 * 1024; // 2MB offset

pub struct PicoFlashStorage {
    flash: Flash<'static, FLASH, Async, FLASH_SIZE>,
}

impl PicoFlashStorage {
    pub fn new(flash: Flash<'static, FLASH, Async, FLASH_SIZE>) -> Self {
        Self { flash }
    }
}

// Implement Storage for PicoFlashStorage
impl Storage for PicoFlashStorage {
    const READ_SIZE: usize = 1;
    const WRITE_SIZE: usize = 256;
    const BLOCK_SIZE: usize = 4096;
    const BLOCK_COUNT: usize = 512;
    const BLOCK_CYCLES: isize = 100;

    type CACHE_SIZE = littlefs2::consts::U256;
    type LOOKAHEAD_SIZE = littlefs2::consts::U4;

    fn read(&mut self, off: usize, buf: &mut [u8]) -> littlefs2::io::Result<usize> {
        self.flash
            .blocking_read(LOG_OFFSET + off as u32, buf)
            .map_err(|_| littlefs2::io::Error::IO)?;
        Ok(buf.len())
    }

    fn write(&mut self, off: usize, data: &[u8]) -> littlefs2::io::Result<usize> {
        self.flash
            .blocking_write(LOG_OFFSET + off as u32, data)
            .map_err(|_| littlefs2::io::Error::IO)?;
        Ok(data.len())
    }

    fn erase(&mut self, off: usize, len: usize) -> littlefs2::io::Result<usize> {
        self.flash
            .blocking_erase(LOG_OFFSET + off as u32, LOG_OFFSET + (off + len) as u32)
            .map_err(|_| littlefs2::io::Error::IO)?;
        Ok(len)
    }
}

pub struct FsWrapper(pub Filesystem<'static, PicoFlashStorage>);
unsafe impl Send for FsWrapper {}

static FLASH_STORAGE: StaticCell<PicoFlashStorage> = StaticCell::new();
static ALLOCATION: StaticCell<Allocation<PicoFlashStorage>> = StaticCell::new();
static FS: Mutex<CriticalSectionRawMutex, Option<FsWrapper>> = Mutex::new(None);
static RTC: Mutex<CriticalSectionRawMutex, Option<AonTimer<'static>>> = Mutex::new(None);

pub fn init<
    I: embassy_rp::interrupt::typelevel::Binding<
            embassy_rp::interrupt::typelevel::POWMAN_IRQ_TIMER,
            embassy_rp::aon_timer::InterruptHandler,
        > + 'static,
>(
    flash: Flash<'static, FLASH, Async, FLASH_SIZE>,
    powman: Peri<'static, POWMAN>,
    irqs: I,
) -> Result<(), littlefs2::io::Error> {
    let storage_ref = FLASH_STORAGE.init(PicoFlashStorage::new(flash));
    let alloc_ref = ALLOCATION.init(Filesystem::allocate());

    if !Filesystem::is_mountable(storage_ref) {
        defmt::warn!("Filesystem not mountable, formatting...");
        Filesystem::format(storage_ref).unwrap();
    }

    let fs = Filesystem::mount(alloc_ref, storage_ref)?;
    if let Ok(mut g) = FS.try_lock() {
        *g = Some(FsWrapper(fs));
    }
    let mut rtc_inst = AonTimer::new(powman, irqs, embassy_rp::aon_timer::Config::default());
    rtc_inst.start();
    if let Ok(mut g) = RTC.try_lock() {
        *g = Some(rtc_inst);
    }

    Ok(())
}

pub async fn set_rtc_time(dt: DateTime) -> Result<(), ()> {
    let mut guard = RTC.lock().await;
    if let Some(rtc) = guard.as_mut() {
        // AonTimer panics on set_datetime if running, so stop first
        rtc.stop();
        let _ = rtc.set_datetime(dt);
        rtc.start();
        Ok(())
    } else {
        Err(())
    }
}

pub async fn get_rtc_time() -> Option<DateTime> {
    if let Ok(mut guard) = RTC.try_lock() {
        if let Some(rtc) = guard.as_mut() {
            return rtc.now_as_datetime().ok();
        }
    }
    None
}

const LOG_FILE: &[u8] = b"syslog.txt\0";
const MAX_LOG_SIZE: usize = 500 * 1024; // 500KB log rotation threshold

pub async fn log_write_all(message: &[u8]) -> Result<(), littlefs2::io::Error> {
    let mut fs_guard = FS.lock().await;
    let fs_locked = match fs_guard.as_mut() {
        Some(fs) => fs,
        None => return Err(littlefs2::io::Error::IO),
    };

    let ms = Instant::now().as_millis();

    // Cstr conversion
    let path = littlefs2::path!("syslog.txt");

    // Check size
    if let Ok(meta) = fs_locked.0.metadata(path)
        && meta.len() > MAX_LOG_SIZE
    {
        let old_path = littlefs2::path!("syslog.0.txt");
        let _ = fs_locked.0.remove(old_path);
        let _ = fs_locked.0.rename(path, old_path);
    }

    // Check for RTC time
    let mut time_str: heapless::String<32> = heapless::String::new();
    if let Ok(mut guard) = RTC.try_lock() {
        if let Some(rtc) = guard.as_mut() {
            if let Ok(dt) = rtc.now_as_datetime() {
                let _ = core::write!(
                    &mut time_str,
                    "[{:04}-{:02}-{:02} {:02}:{:02}:{:02}] ",
                    dt.year,
                    dt.month,
                    dt.day,
                    dt.hour,
                    dt.minute,
                    dt.second
                );
            } else {
                let _ = core::write!(&mut time_str, "[{} ms] ", ms);
            }
        } else {
            let _ = core::write!(&mut time_str, "[{} ms] ", ms);
        }
    } else {
        let _ = core::write!(&mut time_str, "[{} ms] ", ms);
    }

    // Append to log
    fs_locked.0.open_file_with_options_and_then(
        |o| o.write(true).create(true).append(true),
        path,
        |file| {
            file.write(time_str.as_bytes())?;
            file.write(message)?;
            file.write(b"\r\n")?;
            Ok(())
        },
    )?;

    Ok(())
}

pub async fn write_log(msg: &str) -> Result<(), littlefs2::io::Error> {
    log_write_all(msg.as_bytes()).await
}

pub async fn log_clear() -> Result<(), littlefs2::io::Error> {
    let mut fs_guard = FS.lock().await;
    let fs_locked = match fs_guard.as_mut() {
        Some(fs) => fs,
        None => return Ok(()),
    };

    let _ = fs_locked.0.remove(littlefs2::path!("syslog.0.txt"));
    let _ = fs_locked.0.remove(littlefs2::path!("syslog.txt"));
    Ok(())
}

static CWD: Mutex<CriticalSectionRawMutex, heapless::String<256>> =
    Mutex::new(heapless::String::new());

pub async fn pwd() -> heapless::String<256> {
    let mut guard = CWD.lock().await;
    if guard.is_empty() {
        guard.push_str("/").unwrap();
    }
    guard.clone()
}

fn resolve_path_str(cwd: &str, path: &str) -> heapless::String<256> {
    let mut result = heapless::String::<256>::new();

    let full_path = if path.starts_with('/') {
        let mut s = heapless::String::<256>::new();
        s.push_str(path).unwrap_or_default();
        s
    } else {
        let mut s = heapless::String::<256>::new();
        s.push_str(cwd).unwrap_or_default();
        if !s.ends_with('/') {
            s.push('/').unwrap_or_default();
        }
        s.push_str(path).unwrap_or_default();
        s
    };

    let mut parts = heapless::Vec::<&str, 32>::new();

    for part in full_path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        } else if part == ".." {
            let _ = parts.pop();
        } else {
            let _ = parts.push(part);
        }
    }

    result.push('/').unwrap_or_default();
    for (i, part) in parts.iter().enumerate() {
        result.push_str(part).unwrap_or_default();
        if i < parts.len() - 1 {
            result.push('/').unwrap_or_default();
        }
    }

    if result.len() > 1 && result.ends_with('/') {
        result.pop();
    }

    result
}

fn to_lfs_path(p: &str) -> littlefs2::path::PathBuf {
    let p = if p.starts_with('/') && p.len() > 1 {
        &p[1..]
    } else {
        p
    };
    littlefs2::path::PathBuf::try_from(p)
        .unwrap_or_else(|_| littlefs2::path::PathBuf::try_from("/").unwrap())
}

pub async fn fs_mkdir(path: &str) -> Result<(), ()> {
    let cwd_str = pwd().await;
    let resolved = resolve_path_str(cwd_str.as_str(), path);
    let lfs_path = to_lfs_path(&resolved);

    let mut fs_guard = FS.lock().await;
    let fs_locked = match fs_guard.as_mut() {
        Some(fs) => fs,
        None => return Err(()),
    };

    fs_locked.0.create_dir(&lfs_path).map_err(|_| ())?;
    Ok(())
}

pub async fn fs_cd(path: &str) -> Result<(), ()> {
    let cwd_str = pwd().await;
    let resolved = resolve_path_str(cwd_str.as_str(), path);

    let mut fs_guard = FS.lock().await;
    let fs_locked = match fs_guard.as_mut() {
        Some(fs) => fs,
        None => return Err(()),
    };

    if resolved.as_str() != "/" {
        let lfs_path = to_lfs_path(&resolved);
        let meta = fs_locked.0.metadata(&lfs_path).map_err(|_| ())?;
        if !meta.is_dir() {
            return Err(());
        }
    }

    let mut guard = CWD.lock().await;
    guard.clear();
    guard.push_str(&resolved).unwrap();
    Ok(())
}

pub struct DirEntryInfo {
    pub name: heapless::String<64>,
    pub is_dir: bool,
    pub size: usize,
}

pub async fn fs_ls(
    out: &mut crate::cli::CliOutput<'_>,
    path: Option<&str>,
    stack: embassy_net::Stack<'static>,
) -> Result<(), ()> {
    let cwd_str = pwd().await;
    let target_path = path.unwrap_or(cwd_str.as_str());
    let resolved = resolve_path_str(cwd_str.as_str(), target_path);
    let lfs_path = to_lfs_path(&resolved);

    let mut fs_guard = FS.lock().await;
    let fs_locked = match fs_guard.as_mut() {
        Some(fs) => fs,
        None => return Err(()),
    };

    let mut buf = heapless::String::<128>::new();
    core::write!(&mut buf, "Directory of {}:\r\n", resolved.as_str()).ok();
    crate::cli::uart_write_all(out, buf.as_bytes(), stack).await;

    let mut entries = heapless::Vec::<DirEntryInfo, 32>::new();
    let _ = fs_locked.0.read_dir_and_then(&lfs_path, |dir| {
        for entry in dir.flatten() {
            let name = entry.file_name();
            let is_dir = entry.file_type().is_dir();
            // To avoid borrowing issues, entry size is omitted or retrieved via custom struct if needed?
            // littlefs2 DirEntry might not have `metadata()` or `len()`. Let's just retrieve file_type and file_name.
            // It does have `metadata()`.
            let size = entry.metadata().len();

            let mut name_str = heapless::String::<64>::new();
            name_str.push_str(name.as_ref()).unwrap_or_default();

            let _ = entries.push(DirEntryInfo {
                name: name_str,
                is_dir,
                size,
            });
        }
        Ok(())
    });

    for entry in entries {
        let mut line = heapless::String::<128>::new();
        if entry.is_dir {
            core::write!(&mut line, "[DIR]  {}\r\n", entry.name).ok();
        } else {
            core::write!(
                &mut line,
                "[FILE] {} ({} bytes)\r\n",
                entry.name,
                entry.size
            )
            .ok();
        }
        crate::cli::uart_write_all(out, line.as_bytes(), stack).await;
    }
    Ok(())
}

pub async fn fs_cat(
    out: &mut crate::cli::CliOutput<'_>,
    path: &str,
    stack: embassy_net::Stack<'static>,
) -> Result<(), ()> {
    let cwd_str = pwd().await;
    let resolved = resolve_path_str(cwd_str.as_str(), path);
    let lfs_path = to_lfs_path(&resolved);

    let mut fs_guard = FS.lock().await;
    let fs_locked = match fs_guard.as_mut() {
        Some(fs) => fs,
        None => return Err(()),
    };

    let meta = fs_locked.0.metadata(&lfs_path).map_err(|_| ())?;
    if meta.is_dir() {
        let mut buf = heapless::String::<64>::new();
        core::write!(&mut buf, "error: {} is a directory\r\n", path).ok();
        crate::cli::uart_write_all(out, buf.as_bytes(), stack).await;
        return Err(());
    }

    let mut offset = 0;
    let mut buf = [0u8; 128];
    loop {
        let mut bytes_read = 0;
        let res = fs_locked.0.open_file_with_options_and_then(
            |o| o.read(true),
            &lfs_path,
            |file| {
                file.seek(littlefs2::io::SeekFrom::Start(offset as u32))?;
                bytes_read = file.read(&mut buf)?;
                Ok(())
            },
        );

        if res.is_err() {
            let mut buf_str = heapless::String::<64>::new();
            core::write!(&mut buf_str, "error: could not read file\r\n").ok();
            crate::cli::uart_write_all(out, buf_str.as_bytes(), stack).await;
            return Err(());
        }

        if bytes_read == 0 {
            break;
        }

        offset += bytes_read;
        crate::cli::uart_write_all(out, &buf[..bytes_read], stack).await;
    }
    crate::cli::uart_write_all(out, b"\r\n", stack).await;
    Ok(())
}

pub struct WifiConfig {
    pub ssid: heapless::String<64>,
    pub pass: heapless::String<64>,
}

pub async fn write_wifi_conf(ssid: &str, pass: &str) -> Result<(), ()> {
    let mut fs_guard = FS.lock().await;
    let fs_locked = match fs_guard.as_mut() {
        Some(fs) => fs,
        None => return Err(()),
    };

    let path = to_lfs_path("/wifi.conf");
    let mut data = heapless::String::<128>::new();
    let _ = core::write!(&mut data, "{}\n{}", ssid, pass);

    fs_locked
        .0
        .open_file_with_options_and_then(
            |o| o.write(true).create(true).truncate(true),
            &path,
            |file| {
                file.write(data.as_bytes())?;
                Ok(())
            },
        )
        .map_err(|_| ())?;

    Ok(())
}

async fn print_lfs_file(
    fs: &mut Filesystem<'static, PicoFlashStorage>,
    path: &littlefs2::path::Path,
    out: &mut crate::cli::CliOutput<'_>,
    stack: embassy_net::Stack<'static>,
) -> Result<(), ()> {
    let mut buf = [0u8; 512];
    let mut offset = 0;

    loop {
        let mut len = 0;
        let res = fs.open_file_with_options_and_then(
            |o| o.read(true),
            path,
            |file| {
                file.seek(littlefs2::io::SeekFrom::Start(offset))?;
                len = file.read(&mut buf)?;
                Ok(())
            },
        );

        if res.is_err() || len == 0 {
            break;
        }

        crate::cli::uart_write_all(out, &buf[..len], stack).await;
        offset += len as u32;
    }
    Ok(())
}

pub async fn log_print(
    out: &mut crate::cli::CliOutput<'_>,
    stack: embassy_net::Stack<'static>,
) -> Result<(), ()> {
    let mut fs_guard = FS.lock().await;
    let fs_locked = match fs_guard.as_mut() {
        Some(fs) => fs,
        None => return Ok(()),
    };

    // 1. Print rotated log if exists
    let old_path = littlefs2::path!("syslog.0.txt");
    if fs_locked.0.metadata(old_path).is_ok() {
        let _ =
            crate::cli::uart_write_all(out, b"--- Rotated Log (syslog.0.txt) ---\r\n", stack).await;
        let _ = print_lfs_file(&mut fs_locked.0, old_path, out, stack).await;
        let _ = crate::cli::uart_write_all(out, b"--- End of Rotated Log ---\r\n", stack).await;
    }

    // 2. Print current log
    let path = littlefs2::path!("syslog.txt");
    let _ = crate::cli::uart_write_all(out, b"--- Current Log (syslog.txt) ---\r\n", stack).await;
    print_lfs_file(&mut fs_locked.0, path, out, stack).await
}

pub async fn read_wifi_conf() -> Option<WifiConfig> {
    let mut fs_guard = FS.lock().await;
    let fs_locked = match fs_guard.as_mut() {
        Some(fs) => fs,
        None => return None,
    };

    let path = to_lfs_path("/wifi.conf");
    let mut buf = [0u8; 128];
    let mut len = 0;

    let res = fs_locked.0.open_file_with_options_and_then(
        |o| o.read(true),
        &path,
        |file| {
            len = file.read(&mut buf)?;
            Ok(())
        },
    );

    if res.is_err() || len == 0 {
        return None;
    }

    if let Ok(content) = core::str::from_utf8(&buf[..len]) {
        let parts: heapless::Vec<&str, 2> = content.split('\n').collect();
        if parts.len() == 2 {
            let mut ssid = heapless::String::<64>::new();
            let mut pass = heapless::String::<64>::new();
            let _ = ssid.push_str(parts[0].trim());
            let _ = pass.push_str(parts[1].trim());
            return Some(WifiConfig { ssid, pass });
        }
    }
    None
}

pub async fn delete_wifi_conf() -> Result<(), ()> {
    let mut fs_guard = FS.lock().await;
    let fs_locked = match fs_guard.as_mut() {
        Some(fs) => fs,
        None => return Err(()),
    };

    let path = to_lfs_path("/wifi.conf");
    fs_locked.0.remove(&path).map_err(|_| ())
}

pub struct NtpConfig {
    pub server: heapless::String<64>,
}

pub async fn write_ntp_conf(server: &str) -> Result<(), ()> {
    let mut fs_guard = FS.lock().await;
    let fs_locked = match fs_guard.as_mut() {
        Some(fs) => fs,
        None => return Err(()),
    };

    let path = to_lfs_path("/ntp.conf");
    let mut data = heapless::String::<64>::new();
    let _ = data.push_str(server);

    fs_locked
        .0
        .open_file_with_options_and_then(
            |o| o.write(true).create(true).truncate(true),
            &path,
            |file| {
                file.write(data.as_bytes())?;
                Ok(())
            },
        )
        .map_err(|_| ())?;

    Ok(())
}

pub async fn read_ntp_conf() -> Option<NtpConfig> {
    let mut fs_guard = FS.lock().await;
    let fs_locked = match fs_guard.as_mut() {
        Some(fs) => fs,
        None => return None,
    };

    let path = to_lfs_path("/ntp.conf");
    let mut buf = [0u8; 64];
    let mut len = 0;

    let res = fs_locked.0.open_file_with_options_and_then(
        |o| o.read(true),
        &path,
        |file| {
            len = file.read(&mut buf)?;
            Ok(())
        },
    );

    if res.is_err() || len == 0 {
        return None;
    }

    if let Ok(server_str) = core::str::from_utf8(&buf[..len]) {
        let mut server = heapless::String::<64>::new();
        let _ = server.push_str(server_str.trim());
        return Some(NtpConfig { server });
    }
    None
}

pub struct TzConfig {
    pub offset_minutes: i32,
}

pub async fn write_tz_conf(offset_minutes: i32) -> Result<(), ()> {
    let mut fs_guard = FS.lock().await;
    let fs_locked = match fs_guard.as_mut() {
        Some(fs) => fs,
        None => return Err(()),
    };

    let path = to_lfs_path("/timezone.conf");
    let mut data = heapless::String::<16>::new();
    let _ = core::write!(&mut data, "{}", offset_minutes);

    fs_locked
        .0
        .open_file_with_options_and_then(
            |o| o.write(true).create(true).truncate(true),
            &path,
            |file| {
                file.write(data.as_bytes())?;
                Ok(())
            },
        )
        .map_err(|_| ())?;

    Ok(())
}

pub async fn read_tz_conf() -> Option<TzConfig> {
    let mut fs_guard = FS.lock().await;
    let fs_locked = match fs_guard.as_mut() {
        Some(fs) => fs,
        None => return None,
    };

    let path = to_lfs_path("/timezone.conf");
    let mut buf = [0u8; 16];
    let mut len = 0;

    let res = fs_locked.0.open_file_with_options_and_then(
        |o| o.read(true),
        &path,
        |file| {
            len = file.read(&mut buf)?;
            Ok(())
        },
    );

    if res.is_err() || len == 0 {
        return None;
    }

    if let Ok(offset_str) = core::str::from_utf8(&buf[..len]) {
        if let Ok(offset_minutes) = offset_str.trim().parse::<i32>() {
            return Some(TzConfig { offset_minutes });
        }
    }
    None
}
