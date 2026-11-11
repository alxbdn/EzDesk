// Gives EzDesk.exe its icon and version information (what Explorer, Task Manager and the tray
// settings show), by compiling a resource script with the Windows SDK's rc.exe when there is one.
// Without it (a GNU toolchain, no SDK) the exe just has no icon of its own; set
// EZDESK_REQUIRE_RESOURCES=1 to make that an error instead (the release build does).
// EZDESK_VERSION sets the version shown (default: Cargo's).
use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=brand/ezdesk.ico");
    println!("cargo:rerun-if-env-changed=EZDESK_VERSION");
    println!("cargo:rerun-if-env-changed=EZDESK_REQUIRE_RESOURCES");
    let require = env::var("EZDESK_REQUIRE_RESOURCES").map(|v| v == "1").unwrap_or(false);
    let msvc = env::var("CARGO_CFG_TARGET_ENV").map(|v| v == "msvc").unwrap_or(false);
    let fail = |why: &str| {
        if require {
            panic!("EzDesk.exe resources: {why}");
        }
        println!("cargo:warning=EzDesk.exe gets no icon or version info: {why}");
    };
    if !msvc {
        return fail("not an MSVC build");
    }
    let Some(rc) = find_rc() else { return fail("rc.exe (Windows SDK) not found") };

    let version = env::var("EZDESK_VERSION").unwrap_or_else(|_| env::var("CARGO_PKG_VERSION").unwrap());
    let nums: Vec<u16> = version.split(|c: char| !c.is_ascii_digit()).filter_map(|s| s.parse().ok()).chain([0, 0, 0, 0]).take(4).collect();
    let comma = format!("{},{},{},{}", nums[0], nums[1], nums[2], nums[3]);
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let icon = root.join("brand").join("ezdesk.ico").display().to_string().replace('\\', "\\\\");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let script = format!(
        r#"1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {comma}
PRODUCTVERSION {comma}
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "EzDesk"
      VALUE "FileDescription", "EzDesk"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "EzDesk"
      VALUE "LegalCopyright", "MIT License"
      VALUE "OriginalFilename", "EzDesk.exe"
      VALUE "ProductName", "EzDesk"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    );
    let rc_file = out.join("ezdesk.rc");
    let res_file = out.join("ezdesk.res");
    fs::write(&rc_file, script).unwrap();
    let ok = Command::new(&rc).arg("/nologo").arg("/fo").arg(&res_file).arg(&rc_file).status().map(|s| s.success()).unwrap_or(false);
    if !ok {
        return fail("rc.exe failed");
    }
    println!("cargo:rustc-link-arg-bins={}", res_file.display());
}

/// rc.exe on the PATH, else the newest x64 one in the Windows 10/11 SDK.
fn find_rc() -> Option<PathBuf> {
    if Command::new("rc.exe").arg("/?").output().is_ok() {
        return Some(PathBuf::from("rc.exe"));
    }
    let base = PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let mut versions: Vec<PathBuf> = fs::read_dir(&base).ok()?.filter_map(|e| e.ok().map(|e| e.path())).collect();
    versions.sort();
    versions.into_iter().rev().map(|v| v.join("x64").join("rc.exe")).find(|p| p.exists())
}
