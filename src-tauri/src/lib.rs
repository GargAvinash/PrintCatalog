mod print_engine;
mod printer;

use serde::{Deserialize, Serialize};

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
    pub image_id: String,      // reference to image in PrintJob.images
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
    pub images: std::collections::HashMap<String, String>, // id -> base64 data URI
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
async fn cmd_list_printers() -> Result<Vec<PrinterInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || printer::list_printers())
        .await
        .map_err(|e| format!("Runtime error: {}", e))?
}

/// Compose and print a high-quality photo grid directly to a printer.
/// Images are composited at full DPI resolution using the original pixel data,
/// bypassing the browser's print pipeline entirely.
#[tauri::command]
async fn cmd_print_direct(
    window: tauri::Window,
    job: PrintJob,
    printer_name: Option<String>,
) -> Result<String, String> {
    // Run the printing logic in a blocking thread to avoid freezing the main UI thread.
    // Win32 GDI and Print Dialogs are blocking operations.
    tauri::async_runtime::spawn_blocking(move || {
        let printer = printer_name.unwrap_or_else(|| String::from(""));
        
        // Use the window handle for modal dialogs
        #[cfg(target_os = "windows")]
        let hwnd = window.hwnd().ok().map(|h| h.0 as usize);
        #[cfg(not(target_os = "windows"))]
        let hwnd = None;

        printer::print_job(&job, &printer, hwnd)?;
        Ok("Print job sent successfully".to_string())
    })
    .await
    .map_err(|e| format!("Runtime error: {}", e))?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![cmd_list_printers, cmd_print_direct])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
