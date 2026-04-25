mod print_engine;
mod printer;

use serde::{Deserialize, Serialize};
use tauri::Manager;

/// Grid layout configuration from the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GridConfig {
    pub rows: u32,
    pub cols: u32,
    pub cell_width: f64,  // mm
    pub cell_height: f64, // mm
    pub gap_x: f64,       // mm
    pub gap_y: f64,       // mm
    pub padding_top: f64, // mm
    pub padding_left: f64, // mm
    pub page_width: f64,  // mm
    pub page_height: f64, // mm
    pub dpi: u32,
}

/// A single cell placement on the grid
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellInfo {
    pub row: u32,
    pub col: u32,
    pub image_data: String,    // base64 data URI
    pub object_fit: String,    // "cover" or "contain"
    pub alignment: String,     // e.g. "center", "top-left"
    pub rotation: i32,         // 0, 90, 180, 270
    pub outline: bool,
}

/// Full print job sent from the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintJob {
    pub grid: GridConfig,
    pub cells: Vec<CellInfo>,
}

/// Result of listing available printers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterInfo {
    pub name: String,
    pub is_default: bool,
}

/// List available system printers
#[tauri::command]
fn list_printers() -> Result<Vec<PrinterInfo>, String> {
    printer::list_printers()
}

/// Compose and print a high-quality photo grid directly to a printer.
/// Images are composited at full DPI resolution using the original pixel data,
/// bypassing the browser's print pipeline entirely.
#[tauri::command]
fn print_direct(job: PrintJob, printer_name: Option<String>) -> Result<String, String> {
    // 1. Compose the full-resolution page image
    let page_image = print_engine::compose_page(&job)?;

    // 2. Send to printer via Win32 GDI
    let printer = printer_name.unwrap_or_else(|| String::from(""));
    printer::print_image(&page_image, &job.grid, &printer)?;

    Ok("Print job sent successfully".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![list_printers, print_direct])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
