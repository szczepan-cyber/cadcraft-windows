//! System printers: listing them and sending a plot to one.
//!
//! A plotter device with a `printer` (see [`cadcraft_doc::PlotterConfig`]) prints on that
//! printer instead of writing a file; every printer the system has also appears as a device of
//! that name (`plotters::devices`). Linux, macOS and FreeBSD hand the plot to CUPS (`lpstat`,
//! `lp`); Windows prints a PNG through PowerShell and .NET (`print_png.ps1`; a PDF goes to the
//! `PrintTo` verb of its viewer). The printer name is passed as an argument or an environment
//! variable, never spliced into a script. The web build has no system printers. Setting
//! `CADCRAFT_NO_SYSTEM_PRINTERS=1` hides them.

/// Most printers listed.
pub const MAX_PRINTERS: usize = 200;

#[cfg(all(not(target_arch = "wasm32"), windows))]
fn hidden(c: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW: no console window flashes up beside the app.
    c.creation_flags(0x0800_0000)
}

#[cfg(all(not(target_arch = "wasm32"), not(windows)))]
fn hidden(c: &mut std::process::Command) -> &mut std::process::Command {
    c
}

/// The names of the printers the system offers (runs a system tool: may take a second).
#[cfg(not(target_arch = "wasm32"))]
pub fn system_printers() -> std::result::Result<Vec<String>, String> {
    use std::process::Command;
    let out = if cfg!(windows) {
        hidden(Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", "Get-Printer | ForEach-Object { $_.Name }"])).output()
    } else {
        Command::new("lpstat").arg("-e").output()
    }
    .map_err(|e| format!("can't list printers: {e}"))?;
    if !out.status.success() {
        return Err(format!("can't list printers: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let mut names: Vec<String> =
        String::from_utf8_lossy(&out.stdout).lines().map(cadcraft_doc::clean_printer).filter(|n| !n.is_empty()).take(MAX_PRINTERS).collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    Ok(names)
}

#[cfg(target_arch = "wasm32")]
pub fn system_printers() -> std::result::Result<Vec<String>, String> {
    Err("system printers are not available on the web".into())
}

#[cfg(not(target_arch = "wasm32"))]
mod cache {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    struct State {
        at: Option<Instant>,
        names: Vec<String>,
        busy: bool,
    }

    static STATE: Mutex<State> = Mutex::new(State { at: None, names: Vec::new(), busy: false });
    const MAX_AGE: Duration = Duration::from_secs(60);

    /// The printers found so far. The first call (and any call after a minute) starts a
    /// background refresh, so the UI never waits for the system tool.
    pub fn names() -> Vec<String> {
        let Ok(mut st) = STATE.lock() else { return Vec::new() };
        let stale = st.at.is_none_or(|t| t.elapsed() > MAX_AGE);
        if stale && !st.busy {
            st.busy = true;
            let spawned = std::thread::Builder::new().name("cadcraft-printers".into()).spawn(|| {
                let found = super::system_printers().unwrap_or_default();
                if let Ok(mut st) = STATE.lock() {
                    st.names = found;
                    st.at = Some(Instant::now());
                    st.busy = false;
                }
            });
            if spawned.is_err() {
                st.busy = false;
                st.at = Some(Instant::now());
            }
        }
        st.names.clone()
    }
}

/// The system printers known so far, refreshed in the background (empty at first, and always
/// on the web or when `CADCRAFT_NO_SYSTEM_PRINTERS` is set).
pub fn known_printers() -> Vec<String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if std::env::var_os("CADCRAFT_NO_SYSTEM_PRINTERS").is_some_and(|v| !v.is_empty()) {
            return Vec::new();
        }
        cache::names()
    }
    #[cfg(target_arch = "wasm32")]
    Vec::new()
}

/// Send `bytes` (a PDF or PNG file's contents, `ext` is `pdf` or `png`) to the printer called
/// `printer`.
#[cfg(not(target_arch = "wasm32"))]
pub fn print_bytes(printer: &str, bytes: &[u8], ext: &str) -> std::result::Result<(), String> {
    use std::process::Command;
    let printer = cadcraft_doc::clean_printer(printer);
    if printer.is_empty() {
        return Err("the printer name is empty".into());
    }
    if !matches!(ext, "pdf" | "png") {
        return Err("printers print PDF or PNG: set the plotter's output to pdf or png".into());
    }
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let stem = format!("cadcraft-print-{}-{nanos}", std::process::id());
    let file = std::env::temp_dir().join(format!("{stem}.{ext}"));
    std::fs::write(&file, bytes).map_err(|e| format!("can't write the print file: {e}"))?;
    let mut script = None;
    let out = if cfg!(windows) && ext == "png" {
        let ps1 = std::env::temp_dir().join(format!("{stem}.ps1"));
        match std::fs::write(&ps1, include_str!("print_png.ps1")) {
            Ok(()) => {
                let mut c = Command::new("powershell");
                c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]).arg(&ps1);
                c.env("CADCRAFT_PRINT_FILE", &file).env("CADCRAFT_PRINTER", &printer);
                script = Some(ps1);
                hidden(&mut c).output()
            }
            Err(e) => Err(e),
        }
    } else if cfg!(windows) {
        let mut c = Command::new("powershell");
        c.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Start-Process -FilePath $env:CADCRAFT_PRINT_FILE -Verb PrintTo -ArgumentList ('\"' + $env:CADCRAFT_PRINTER + '\"') -WindowStyle Hidden",
        ]);
        c.env("CADCRAFT_PRINT_FILE", &file).env("CADCRAFT_PRINTER", &printer);
        hidden(&mut c).output()
    } else {
        Command::new("lp").arg("-d").arg(&printer).arg("-t").arg("CADCraft").arg("--").arg(&file).output()
    };
    // CUPS has spooled the file and the .NET print has finished when the tool returns; the
    // PrintTo handler of a PDF may still be reading it, so that one is left to the temp folder.
    if !(cfg!(windows) && ext == "pdf") {
        let _ = std::fs::remove_file(&file);
    }
    if let Some(s) = script {
        let _ = std::fs::remove_file(s);
    }
    let out = out.map_err(|e| format!("can't print: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let msg = String::from_utf8_lossy(&out.stderr);
        Err(format!("printing on `{printer}` failed: {}", msg.lines().next().unwrap_or("").trim()))
    }
}

#[cfg(target_arch = "wasm32")]
pub fn print_bytes(_printer: &str, _bytes: &[u8], _ext: &str) -> std::result::Result<(), String> {
    Err("system printers are not available on the web".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_printer_input_is_an_error_not_a_panic() {
        assert!(print_bytes("", b"x", "pdf").is_err());
        assert!(print_bytes("-o evil", b"x", "pdf").is_err());
        assert!(print_bytes("Some Printer", b"x", "svg").is_err());
        // Without CUPS or PowerShell (or without such a printer) this fails cleanly.
        let _ = system_printers();
        assert!(print_bytes("No Such Printer \u{0}", b"x", "pdf").is_err() || cfg!(windows));
    }

    /// Windows runners have "Microsoft Print to PDF": list it and print a PNG to a file with it.
    #[cfg(windows)]
    #[test]
    fn windows_lists_and_prints_to_a_virtual_printer() {
        let names = system_printers().expect("Get-Printer");
        println!("printers: {names:?}");
        let Some(p) = names.iter().find(|n| n.eq_ignore_ascii_case("Microsoft Print to PDF")) else { return };
        let png = std::env::temp_dir().join("cadcraft-test.png");
        let mut img = Vec::new();
        // A valid 1x1 white PNG.
        img.extend_from_slice(&[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
            0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xFF,
            0xFF, 0x3F, 0x00, 0x05, 0xFE, 0x02, 0xFE, 0x0D, 0xEF, 0x46, 0xB8, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ]);
        std::fs::write(&png, &img).expect("write");
        let out = std::env::temp_dir().join("cadcraft-test-out.pdf");
        let ps1 = std::env::temp_dir().join("cadcraft-test.ps1");
        std::fs::write(&ps1, include_str!("print_png.ps1")).expect("script");
        let o = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&ps1)
            .env("CADCRAFT_PRINT_FILE", &png)
            .env("CADCRAFT_PRINTER", p)
            .env("CADCRAFT_PRINT_TO_FILE", &out)
            .output()
            .expect("powershell");
        println!("stderr: {}", String::from_utf8_lossy(&o.stderr));
        assert!(o.status.success());
        assert!(out.exists(), "the virtual printer wrote a file");
    }
}
