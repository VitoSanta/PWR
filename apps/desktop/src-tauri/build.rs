fn main() {
    for key in [
        "PWR_UPDATER_PUBLIC_KEY",
        "PWR_UPDATER_ENDPOINT",
        "PWR_UPDATER_CHANNEL",
    ] {
        println!("cargo:rerun-if-env-changed={key}");
    }
    tauri_build::build()
}
