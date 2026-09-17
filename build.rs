use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=assets/everytree.ico");
    println!("cargo:rerun-if-env-changed=RC");
    println!("cargo:rerun-if-env-changed=WindowsSdkDir");
    println!("cargo:rerun-if-env-changed=WindowsSdkVerBinPath");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    assert_eq!(
        env::var("CARGO_CFG_TARGET_ENV").as_deref(),
        Ok("msvc"),
        "The Windows build requires the Rust MSVC toolchain and Windows SDK."
    );
    let assets = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("assets");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let rc = output.join("everytree.rc");
    let resource = output.join("everytree.res");
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let version_numbers = format!(
        "{},{},{},0",
        env::var("CARGO_PKG_VERSION_MAJOR").unwrap(),
        env::var("CARGO_PKG_VERSION_MINOR").unwrap(),
        env::var("CARGO_PKG_VERSION_PATCH").unwrap()
    );
    fs::write(
        &rc,
        format!(
            r#"
1 ICON "everytree.ico"
1 VERSIONINFO
FILEVERSION {version_numbers}
PRODUCTVERSION {version_numbers}
FILEFLAGSMASK 0x3fL
FILEFLAGS 0
FILEOS 0x40004L
FILETYPE 1
FILESUBTYPE 0
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "FileDescription", "everytree\0"
      VALUE "FileVersion", "{version}\0"
      VALUE "InternalName", "everytree\0"
      VALUE "OriginalFilename", "everytree.exe\0"
      VALUE "ProductName", "everytree\0"
      VALUE "ProductVersion", "{version}\0"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0409, 1200
  END
END
"#
        ),
    )
    .expect("Could not write Windows resource description");
    let compiler = resource_compiler();
    let result = Command::new(&compiler)
        .current_dir(&assets)
        .arg("/nologo")
        .arg("/fo")
        .arg(&resource)
        .arg(&rc)
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "Could not run {}: {e}. Install the Windows SDK or set RC to its rc.exe path.",
                compiler.display()
            )
        });
    assert!(
        result.status.success(),
        "Windows resource compilation failed: {}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    println!("cargo:rustc-link-arg-bin=everytree={}", resource.display());
}

fn resource_compiler() -> PathBuf {
    if let Some(path) = env::var_os("RC") {
        return path.into();
    }
    if let Some(bin) = env::var_os("WindowsSdkVerBinPath") {
        let path = PathBuf::from(bin).join("x64/rc.exe");
        if path.is_file() {
            return path;
        }
    }
    let sdk = env::var_os("WindowsSdkDir")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                env::var_os("ProgramFiles(x86)")
                    .unwrap_or_else(|| r"C:\Program Files (x86)".into()),
            )
            .join("Windows Kits/10")
        });
    let bin = sdk.join("bin");
    let mut versions: Vec<_> = fs::read_dir(&bin)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    versions.sort_by_cached_key(|p| {
        p.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .split('.')
            .filter_map(|v| v.parse::<u32>().ok())
            .collect::<Vec<_>>()
    });
    for version in versions.into_iter().rev() {
        let path = version.join("x64/rc.exe");
        if path.is_file() {
            return path;
        }
    }
    let fallback = bin.join("x64/rc.exe");
    if fallback.is_file() {
        fallback
    } else {
        "rc.exe".into()
    }
}
