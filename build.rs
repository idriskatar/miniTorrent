// exe dosyasına simgeyi gömer (yalnızca Windows'ta)
fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "MiniTorrent");
        res.set("FileDescription", "MiniTorrent");
        if let Err(e) = res.compile() {
            println!("cargo:warning=simge eklenemedi: {e}");
        }
    }
}
