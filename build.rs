fn main() {
    println!("cargo:rerun-if-changed=assets/aura.ico");

    #[cfg(windows)]
    {
        if std::path::Path::new("assets/aura.ico").exists() {
            let mut res = winres::WindowsResource::new();
            res.set_icon("assets/aura.ico");
            res.set("ProductName", "Aura Engine");
            res.set("FileDescription", "Aura Engine - Native Live Wallpaper Manager");
            res.set("LegalCopyright", "GPL-3.0-or-later");
            res.set("OriginalFilename", "aura.exe");
            if let Err(err) = res.compile() {
                println!("cargo:warning=winres failed: {err}");
            }
        }
    }
}
