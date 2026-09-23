use std::env;

fn main() {
    tauri_build::build();

    if env::var_os("CARGO_CFG_WINDOWS").is_some() {
        // rfd imports TaskDialogIndirect from ComCtl32 v6; delay loading keeps test
        // binaries startable on Windows 10's default v5 activation context.
        println!("cargo:rustc-link-arg=/DELAYLOAD:comctl32.dll");
        println!("cargo:rustc-link-lib=dylib=delayimp");
        println!("cargo:rerun-if-changed=build.rs");
    }
}
