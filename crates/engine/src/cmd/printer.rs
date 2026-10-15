//! System printers: listing them and sending a plot to one.
//!
//! A plotter device with a `printer` (see [`cadcraft_doc::PlotterConfig`]) prints on that
//! printer instead of writing a file. Linux, macOS and FreeBSD hand the plot to CUPS (`lpstat`,
//! `lp`); Windows uses PowerShell (`Get-Printer`, the `PrintTo` verb of the file's handler).
//! The printer name is passed as an argument or an environment variable, never spliced into a
//! script. The web build has no system printers.

/// Most printers listed.
pub const MAX_PRINTERS: usize = 200;

/// The names of the printers the system offers.
#[cfg(not(target_arch = "wasm32"))]
pub fn system_printers() -> std::result::Result<Vec<String>, String> {
    use std::process::Command;
    let out = if cfg!(windows) {
        Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", "Get-Printer | ForEach-Object { $_.Name }"]).output()
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
    let file = std::env::temp_dir().join(format!("cadcraft-print-{}-{nanos}.{ext}", std::process::id()));
    std::fs::write(&file, bytes).map_err(|e| format!("can't write the print file: {e}"))?;
    let out = if cfg!(windows) {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Process -FilePath $env:CADCRAFT_PRINT_FILE -Verb PrintTo -ArgumentList ('\"' + $env:CADCRAFT_PRINTER + '\"') -WindowStyle Hidden",
            ])
            .env("CADCRAFT_PRINT_FILE", &file)
            .env("CADCRAFT_PRINTER", &printer)
            .output()
    } else {
        Command::new("lp").arg("-d").arg(&printer).arg("-t").arg("CADCraft").arg("--").arg(&file).output()
    };
    // CUPS has spooled the file when `lp` returns; the Windows handler may still be reading it,
    // so that one is left to the system's temporary folder.
    if !cfg!(windows) {
        let _ = std::fs::remove_file(&file);
    }
    let out = out.map_err(|e| format!("can't print: {e}"))?;
    if out.status.success() { Ok(()) } else { Err(format!("printing on `{printer}` failed: {}", String::from_utf8_lossy(&out.stderr).trim())) }
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
        let _ = print_bytes("No Such Printer \u{0}", b"x", "pdf");
    }
}
