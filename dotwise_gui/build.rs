fn main() {
    // Windows: embed the icon into the .exe so Explorer, the taskbar and shortcuts show it.
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("../assets/dotwise.ico");
        res.set("ProductName", "Dotwise");
        res.set("FileDescription", "Dotwise");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the icon: {}", e);
        }
    }
    println!("cargo:rerun-if-changed=../assets/dotwise.ico");
}
