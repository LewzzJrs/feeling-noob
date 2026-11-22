fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource
            .set_icon("assets/icon.ico")
            .set("ProductName", "feeling-noob")
            .set("FileDescription", "feeling-noob")
            .set("OriginalFilename", "fnoob-app.exe");
        if let Err(e) = resource.compile() {
            println!("cargo:warning=icon not embedded: {e}");
        }
    }
}
