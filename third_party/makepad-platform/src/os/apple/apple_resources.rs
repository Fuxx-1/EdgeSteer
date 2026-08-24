use {
    std::{
        rc::Rc,
        io::prelude::*,
        fs::File,
    },
    crate::{
        os::{
            apple::apple_sys::*,
            apple::apple_util::nsstring_to_string,
        },
        cx::Cx,
    }
};

impl Cx {
    /// Loads resources as dependencies from the NSBundle's resource path.
    ///
    /// This is used for any Apple app bundle on iOS, macOS, or tvOS.
    #[allow(unused)]
    pub(crate) fn apple_bundle_load_dependencies(&mut self) {
        let bundle_path = unsafe{
            let main:ObjcId = msg_send![class!(NSBundle), mainBundle];
            let path:ObjcId = msg_send![main, resourcePath];
            nsstring_to_string(path)
        };

        for (path,dep) in &mut self.dependencies{
            // Live compiler paths contain the build machine's absolute Cargo
            // directories. Release bundles carry the same files below their
            // `resources/` subtree, so resolve that stable suffix at runtime.
            let mut file_handle = crate::os::cx_native::resource_candidates(path)
                .into_iter()
                .map(File::open)
                .find_map(Result::ok);
            if file_handle.is_none() {
                let bundled_path = path
                    .split_once("resources/")
                    .map(|(_, suffix)| format!("{}/resources/{}", bundle_path, suffix));
                file_handle = bundled_path.as_deref().and_then(|path| File::open(path).ok());
            }
            if let Some(mut file_handle) = file_handle {
                let mut buffer = Vec::<u8>::new();
                if file_handle.read_to_end(&mut buffer).is_ok() {
                    dep.data = Some(Ok(Rc::new(buffer)));
                }
                else{
                    dep.data = Some(Err("read_to_end failed".to_string()));
                }
            }
            else{
                dep.data = Some(Err("Bundled file open failed".to_string()));
            }
        }
    }
}
