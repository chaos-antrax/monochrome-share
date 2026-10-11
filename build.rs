use std::{env, path::PathBuf, process::Command};

fn main() {
    for source in [
        "logo.png",
        "scripts/Build-BrandAssets.ps1",
        "packaging/desktop.rc",
    ] {
        println!("cargo:rerun-if-changed={source}");
    }
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory"));
    let status = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            "scripts/Build-BrandAssets.ps1",
            "-OutputDirectory",
        ])
        .arg(&output)
        .arg("-CompileResource")
        .status()
        .expect("Run Windows branding resource builder");
    assert!(status.success(), "Windows branding resource build failed");
    println!(
        "cargo:rustc-link-arg-bin=desktop={}",
        output.join("desktop.res").display()
    );
}
