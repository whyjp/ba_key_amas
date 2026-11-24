// Gives ba-key-AMAS.exe the project icon (Windows only; other targets skip it).
fn main() {
    println!("cargo:rerun-if-changed=../../assets/ba-key-amas.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/ba-key-amas.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icon not embedded: {e}");
        }
    }
}
