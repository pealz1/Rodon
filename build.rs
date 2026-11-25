fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=Cargo.toml");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/icon.ico")
            .set("ProductName", "Rodon")
            .set("FileDescription", "Rodon — Roblox account manager")
            .set("OriginalFilename", "Rodon.exe")
            .set("Comments", "Based on Evanovar RAM by evanovar and Roblox Account Manager by ic3w0lf22.")
            .compile_for("Rodon")
            .expect("embed Rodon icon and Windows version information");
    }
}
