use {
    std::{
        io::prelude::*,
        fs::File,
        path::{Path, PathBuf},
        rc::Rc,
        time::{SystemTime}
    },
    crate::{
        cx::{Cx},
    }
};

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum EventFlow{
    Poll,
    Wait,
    Exit
}

// lets start a websocket thread


impl Cx {
    
    pub fn native_load_dependencies(&mut self){
        for (path,dep) in &mut self.dependencies{
            let mut file_handle = Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "resource candidates not found",
            ));
            for candidate in resource_candidates(path) {
                if let Ok(file) = File::open(&candidate) {
                    file_handle = Ok(file);
                    break;
                }
            }
            if file_handle.is_err() {
                file_handle = File::open(path);
            }
            if let Ok(mut file_handle) = file_handle {
                let mut buffer = Vec::<u8>::new();
                if file_handle.read_to_end(&mut buffer).is_ok() {
                    dep.data = Some(Ok(Rc::new(buffer)));
                }
                else{
                    dep.data = Some(Err("read_to_end failed".to_string()));
                }
            }
            else{
                println!("Could not load resource {}", path);
                dep.data = Some(Err("File! open failed".to_string()));
            }
        }
    }
    
    pub fn time_now()->f64{
        if let Ok(elapsed) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH){
            return elapsed.as_secs_f64();
        }
        return 0.0
    }
}

pub(crate) fn resource_candidates(path: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let file_name = Path::new(path).file_name().and_then(|name| name.to_str());

    if let Some(file_name) = file_name {
        if is_font_resource(file_name) {
            candidates.extend(
                system_font_candidates(file_name)
                    .into_iter()
                    .filter(|candidate| font_file_is_usable(candidate)),
            );
            if let Some(cache) = font_cache_dir() {
                let cached = cache.join(file_name);
                if font_file_is_usable(&cached) {
                    candidates.push(cached);
                }
            }
        }
    }

    if let Some((_, suffix)) = path.split_once("resources/") {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let bundled = dir.join("resources").join(suffix);
                if !is_font_resource(file_name.unwrap_or_default())
                    || font_file_is_usable(&bundled)
                {
                    candidates.push(bundled);
                }
                if let Some(contents) = dir.parent() {
                    let bundled = contents.join("Resources").join("resources").join(suffix);
                    if !is_font_resource(file_name.unwrap_or_default())
                        || font_file_is_usable(&bundled)
                    {
                        candidates.push(bundled);
                    }
                }
            }
        }
    }
    candidates
}

fn is_font_resource(file_name: &str) -> bool {
    file_name.ends_with(".ttf")
        || file_name.ends_with(".ttf.2")
        || file_name.ends_with(".ttc")
}

// Makepad's font parser requires a complete sfnt file. This rejects truncated
// remote downloads before they can become the first resource candidate.
fn font_file_is_usable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    if path.extension().is_some_and(|extension| extension == "ttc") {
        return true;
    }
    let Ok(data) = std::fs::read(path) else {
        return false;
    };
    if data.len() < 12 {
        return false;
    }
    let table_count = u16::from_be_bytes([data[4], data[5]]) as usize;
    let records_end = 12usize.saturating_add(table_count.saturating_mul(16));
    if records_end > data.len() {
        return false;
    }
    (0..table_count).all(|index| {
        let offset = 12 + index * 16;
        let table_start = u32::from_be_bytes([
            data[offset + 8],
            data[offset + 9],
            data[offset + 10],
            data[offset + 11],
        ]) as usize;
        let table_length = u32::from_be_bytes([
            data[offset + 12],
            data[offset + 13],
            data[offset + 14],
            data[offset + 15],
        ]) as usize;
        table_start
            .checked_add(table_length)
            .is_some_and(|end| end <= data.len())
    })
}

fn font_cache_dir() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("EDGESTEER_FONT_CACHE") {
        return Some(PathBuf::from(path));
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return Some(PathBuf::from(home).join("Library/Caches/EdgeSteer/fonts"));
    }
    #[cfg(target_os = "windows")]
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        return Some(PathBuf::from(local_app_data).join("EdgeSteer/fonts"));
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".cache/edgesteer/fonts"))
}

fn system_font_candidates(file_name: &str) -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    let _ = file_name;
    #[cfg(not(target_os = "macos"))]
    let names: &[&str] = if file_name.contains("NotoColorEmoji") {
        &["NotoColorEmoji.ttf", "seguiemj.ttf"]
    } else {
        &[
            "LXGWWenKai-Regular.ttf",
            "LXGWWenKai-Medium.ttf",
            "Arial Unicode.ttf",
            "msyh.ttc",
            "NotoSansCJK-Regular.ttc",
        ]
    };
    let mut paths = Vec::new();
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(PathBuf::from(home.clone()).join("Library/Fonts/LXGWWenKai-Regular.ttf"));
        paths.push(PathBuf::from(home).join("Library/Fonts/LXGWWenKai-Medium.ttf"));
    }
    #[cfg(target_os = "windows")]
    paths.extend(names.iter().map(|name| Path::new("C:/Windows/Fonts").join(name)));
    #[cfg(target_os = "linux")]
    for directory in ["/usr/share/fonts", "/usr/local/share/fonts"] {
        paths.extend(names.iter().map(|name| Path::new(directory).join(name)));
    }
    paths
}
