// build.rs

fn main() {
    // Esta línea es la clave: solo ejecuta el código si el SO de destino es Windows.
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("icon.ico"); // Asume que tienes un icon.ico
        res.compile().expect("Fallo al compilar los recursos de Windows.");
    }
}
