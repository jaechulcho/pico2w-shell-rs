#![allow(dead_code)]

use core::fmt::Write as _;
use defmt::{error, info, warn};
use embassy_rp::flash::{Async, Flash};
use embassy_rp::peripherals::FLASH;
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

pub fn init(flash: Flash<'static, FLASH, Async, FLASH_SIZE>) -> Result<(), littlefs2::io::Error> {
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
    Ok(())
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
    if let Ok(meta) = fs_locked.0.metadata(path) {
        if meta.len() > MAX_LOG_SIZE {
            let old_path = littlefs2::path!("syslog.0.txt");
            let _ = fs_locked.0.remove(old_path);
            let _ = fs_locked.0.rename(path, old_path);
        }
    }

    // Append to log
    fs_locked.0.open_file_with_options_and_then(
        |o| o.write(true).create(true).append(true),
        path,
        |file| {
            let mut line_buf: heapless::String<256> = heapless::String::new();
            core::write!(&mut line_buf, "[{} ms] ", ms).ok();

            file.write(line_buf.as_bytes())?;
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

pub async fn log_print(
    uart: &mut embassy_rp::uart::Uart<'static, embassy_rp::uart::Async>,
) -> Result<(), littlefs2::io::Error> {
    let mut fs_guard = FS.lock().await;
    let fs_locked = match fs_guard.as_mut() {
        Some(fs) => fs,
        None => return Ok(()),
    };

    let paths = [
        littlefs2::path!("syslog.0.txt"),
        littlefs2::path!("syslog.txt"),
    ];

    for path in &paths {
        if let Ok(_) = fs_locked.0.metadata(*path) {
            let mut offset = 0;
            let mut buf = [0u8; 128];
            loop {
                let mut bytes_read = 0;
                let _ = fs_locked.0.open_file_with_options_and_then(
                    |o| o.read(true),
                    *path,
                    |file| {
                        file.seek(littlefs2::io::SeekFrom::Start(offset as u32))?;
                        bytes_read = file.read(&mut buf)?;
                        Ok(())
                    },
                );

                if bytes_read == 0 {
                    break;
                }

                offset += bytes_read;
                crate::cli::uart_write_all(uart, &buf[..bytes_read]).await;
            }
        }
    }

    Ok(())
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
        if part == "" || part == "." {
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
    uart: &mut embassy_rp::uart::Uart<'static, embassy_rp::uart::Async>,
    path: Option<&str>,
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
    crate::cli::uart_write_all(uart, buf.as_bytes()).await;

    let mut entries = heapless::Vec::<DirEntryInfo, 32>::new();
    let _ = fs_locked.0.read_dir_and_then(&lfs_path, |dir| {
        for entry_res in dir {
            if let Ok(entry) = entry_res {
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
        crate::cli::uart_write_all(uart, line.as_bytes()).await;
    }
    Ok(())
}

pub async fn fs_cat(
    uart: &mut embassy_rp::uart::Uart<'static, embassy_rp::uart::Async>,
    path: &str,
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
        crate::cli::uart_write_all(uart, buf.as_bytes()).await;
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
            crate::cli::uart_write_all(uart, buf_str.as_bytes()).await;
            return Err(());
        }

        if bytes_read == 0 {
            break;
        }

        offset += bytes_read;
        crate::cli::uart_write_all(uart, &buf[..bytes_read]).await;
    }
    crate::cli::uart_write_all(uart, b"\r\n").await;
    Ok(())
}
