use {
    std::{
        io::prelude::*,
        fs::File,
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
            let bundled_paths = path.split_once("resources/").and_then(|(_, suffix)| {
                std::env::current_exe().ok().map(|exe| {
                    let mut paths = Vec::with_capacity(2);
                    if let Some(dir) = exe.parent() {
                        paths.push(dir.join("resources").join(suffix));
                        // macOS app bundles place files under Contents/Resources,
                        // while the executable itself lives in Contents/MacOS.
                        if let Some(contents) = dir.parent() {
                            paths.push(contents.join("Resources").join("resources").join(suffix));
                        }
                    }
                    paths
                })
            });
            let mut file_handle = File::open(path);
            if file_handle.is_err() {
                if let Some(paths) = bundled_paths {
                    for candidate in paths {
                        if let Ok(file) = File::open(candidate) {
                            file_handle = Ok(file);
                            break;
                        }
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
