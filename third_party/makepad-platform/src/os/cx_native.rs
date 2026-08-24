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
            let mut file_handle = File::open(path);
            if file_handle.is_err() {
                for candidate in resource_candidates(path) {
                    if let Ok(file) = File::open(candidate) {
                        file_handle = Ok(file);
                        break;
                    }
                }
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
            candidates.extend(system_font_candidates(file_name));
            if let Some(cache) = font_cache_dir() {
                candidates.push(cache.join(file_name));
            }
        }
    }

    if let Some((_, suffix)) = path.split_once("resources/") {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                candidates.push(dir.join("resources").join(suffix));
                if let Some(contents) = dir.parent() {
                    candidates.push(contents.join("Resources").join("resources").join(suffix));
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
    let names: &[&str] = if file_name.contains("NotoColorEmoji") {
        &["NotoColorEmoji.ttf", "Apple Color Emoji.ttc", "seguiemj.ttf"]
    } else {
        &[
            "LXGWWenKai-Regular.ttf",
            "LXGWWenKai-Medium.ttf",
            "PingFang.ttc",
            "PingFang SC.ttc",
            "Hiragino Sans GB.ttc",
            "STHeiti Medium.ttc",
            "STHeiti Light.ttc",
            "Songti.ttc",
            "msyh.ttc",
            "NotoSansCJK-Regular.ttc",
        ]
    };
    let mut paths = Vec::new();
    #[cfg(target_os = "macos")]
    for directory in ["/System/Library/Fonts", "/System/Library/Fonts/Supplemental", "/Library/Fonts"] {
        paths.extend(names.iter().map(|name| Path::new(directory).join(name)));
    }
    #[cfg(target_os = "windows")]
    paths.extend(names.iter().map(|name| Path::new("C:/Windows/Fonts").join(name)));
    #[cfg(target_os = "linux")]
    for directory in ["/usr/share/fonts", "/usr/local/share/fonts"] {
        paths.extend(names.iter().map(|name| Path::new(directory).join(name)));
    }
    paths
}
