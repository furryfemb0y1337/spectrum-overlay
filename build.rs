use std::{env, fs, path::PathBuf, process::Command};

fn run(command: &mut Command) {
    let status = command
        .status()
        .unwrap_or_else(|e| panic!("{command:?}: {e}"));
    assert!(status.success(), "{command:?}: {status}");
}

fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/spectrum.ico");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let object = out.join("spectrum-icon.obj");
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu")
        && Command::new("windres").arg("--version").output().is_ok()
    {
        run(Command::new("windres")
            .current_dir("assets")
            .args(["-i", "app.rc", "-O", "coff", "-o"])
            .arg(&object));
    } else {
        let sdk = PathBuf::from(env::var_os("ProgramFiles(x86)").expect("Windows SDK required"))
            .join("Windows Kits/10/bin");
        let rc = fs::read_dir(sdk)
            .expect("Install Windows SDK or MinGW windres")
            .flatten()
            .map(|entry| entry.path().join("x64/rc.exe"))
            .filter(|path| path.is_file())
            .max()
            .expect("Windows SDK rc.exe required");
        let res = out.join("spectrum-icon.res");
        run(Command::new(rc)
            .current_dir("assets")
            .args(["/nologo", "/fo"])
            .arg(&res)
            .arg("app.rc"));
        let converter = PathBuf::from(env::var_os("WINDIR").unwrap())
            .join("Microsoft.NET/Framework64/v4.0.30319/cvtres.exe");
        let machine = match env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
            "x86_64" => "X64",
            "x86" => "X86",
            "aarch64" => "ARM64",
            arch => panic!("Unsupported Windows architecture: {arch}"),
        };
        run(Command::new(converter)
            .arg("/NOLOGO")
            .arg(format!("/MACHINE:{machine}"))
            .arg(format!("/OUT:{}", object.display()))
            .arg(res));
    }
    println!("cargo:rustc-link-arg={}", object.display());
}
