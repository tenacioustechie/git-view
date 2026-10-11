fn main() {
    let mut attributes = tauri_build::Attributes::new();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let manifest = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .join("tests/windows.manifest");
        println!("cargo:rerun-if-changed=tests/windows.manifest");
        // Cargo treats the library unit-test harness as a library target even
        // though rustc links it as an executable. The `-tests` directive only
        // reached the binary/integration harnesses, leaving this executable
        // without the Common Controls v6 manifest and unable to load its
        // comctl32 imports. The general link argument reaches every final
        // executable. Tauri's default Windows manifest contains this same
        // dependency and is normally linked only into app binaries, so disable
        // that copy to avoid duplicate resource ID 1 when the general linker
        // input reaches those binaries too.
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    tauri_build::try_build(attributes).expect("failed to run Tauri build script")
}
