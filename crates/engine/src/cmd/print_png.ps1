# Prints a PNG (the plotted sheet) on a Windows printer, fitted to the page and centred.
# Environment: CADCRAFT_PRINT_FILE (the PNG), CADCRAFT_PRINTER (printer name),
# CADCRAFT_PRINT_TO_FILE (optional: print to this file instead, for tests).
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$img = [System.Drawing.Image]::FromFile($env:CADCRAFT_PRINT_FILE)
try {
    $pd = New-Object System.Drawing.Printing.PrintDocument
    $pd.PrinterSettings.PrinterName = $env:CADCRAFT_PRINTER
    if (-not $pd.PrinterSettings.IsValid) { throw ('printer not found: ' + $env:CADCRAFT_PRINTER) }
    if ($env:CADCRAFT_PRINT_TO_FILE) {
        $pd.PrinterSettings.PrintToFile = $true
        $pd.PrinterSettings.PrintFileName = $env:CADCRAFT_PRINT_TO_FILE
    }
    $pd.DocumentName = 'CADCraft'
    $pd.DefaultPageSettings.Landscape = ($img.Width -gt $img.Height)
    $pd.add_PrintPage({
        param($sender, $e)
        $r = $e.PageBounds
        $k = [Math]::Min($r.Width / [double]$img.Width, $r.Height / [double]$img.Height)
        $w = [single]($img.Width * $k)
        $h = [single]($img.Height * $k)
        $x = [single]($r.Left + ($r.Width - $w) / 2)
        $y = [single]($r.Top + ($r.Height - $h) / 2)
        $e.Graphics.DrawImage($img, $x, $y, $w, $h)
        $e.HasMorePages = $false
    })
    $pd.Print()
}
finally {
    $img.Dispose()
}
