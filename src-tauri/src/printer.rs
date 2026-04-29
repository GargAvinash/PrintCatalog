/// Win32 Printer Integration — Direct OS printing via GDI.
///
/// Sends original-resolution images directly to the printer.
/// The printer driver handles all DPI mapping and scaling.
/// Uses source-crop StretchDIBits for cover/contain and GDI vector outlines.
use crate::{GridConfig, PrintJob, PrinterInfo};
use std::ffi::CString;
use windows::Win32::Foundation::{COLORREF, GlobalFree, HGLOBAL};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::Printing::*;
use windows::Win32::Storage::Xps::{DOCINFOA, EndDoc, EndPage, StartDocA, StartPage};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::UI::Controls::Dialogs::{
    CommDlgExtendedError, DEVNAMES, PD_HIDEPRINTTOFILE, PD_NOPAGENUMS, PD_NOSELECTION, PD_RETURNDC,
    PRINTDLGA, PrintDlgA,
};
use windows::core::{PCSTR, PSTR};

/// List all available printers on the system
pub fn list_printers() -> Result<Vec<PrinterInfo>, String> {
    unsafe {
        let mut bytes_needed: u32 = 0;
        let mut count: u32 = 0;

        let _ = EnumPrintersA(
            PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS,
            PCSTR::null(),
            2,
            None,
            &mut bytes_needed,
            &mut count,
        );

        if bytes_needed == 0 {
            return Ok(vec![]);
        }

        let mut buffer = vec![0u8; bytes_needed as usize];

        let result = EnumPrintersA(
            PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS,
            PCSTR::null(),
            2,
            Some(&mut buffer),
            &mut bytes_needed,
            &mut count,
        );

        if result.is_err() {
            return Err("Failed to enumerate printers".to_string());
        }

        let mut default_name_buf = vec![0u8; 512];
        let mut default_size = default_name_buf.len() as u32;
        let has_default =
            GetDefaultPrinterA(Some(PSTR(default_name_buf.as_mut_ptr())), &mut default_size).0 != 0;
        let default_name = if has_default {
            let end = default_name_buf.iter().position(|&b| b == 0).unwrap_or(0);
            String::from_utf8_lossy(&default_name_buf[..end]).to_string()
        } else {
            String::new()
        };

        let info_ptr = buffer.as_ptr() as *const PRINTER_INFO_2A;
        let mut printers = Vec::new();

        for i in 0..count as isize {
            let info = &*info_ptr.offset(i);
            let name = if !info.pPrinterName.0.is_null() {
                let c_str = std::ffi::CStr::from_ptr(info.pPrinterName.0 as *const i8);
                c_str.to_string_lossy().to_string()
            } else {
                continue;
            };

            printers.push(PrinterInfo {
                is_default: name == default_name,
                name,
            });
        }

        Ok(printers)
    }
}

/// Convert RGBA image to BGR bottom-up byte buffer for GDI
fn rgba_to_bgr(img: &image::RgbaImage) -> Vec<u8> {
    let w = img.width() as usize;
    let h = img.height() as usize;
    let row_bytes = w * 3;
    let padding = (4 - (row_bytes % 4)) % 4;
    let stride = row_bytes + padding;

    let mut bgr = vec![0u8; stride * h];
    let raw = img.as_raw();

    for y in 0..h {
        let src_y = h - 1 - y;
        let dest_row_start = y * stride;
        let src_row_start = src_y * w * 4;

        for x in 0..w {
            let src_px = src_row_start + x * 4;
            let dest_px = dest_row_start + x * 3;
            bgr[dest_px] = raw[src_px + 2]; // B
            bgr[dest_px + 1] = raw[src_px + 1]; // G
            bgr[dest_px + 2] = raw[src_px]; // R
        }
    }
    bgr
}

fn alignment_factors(alignment: &str) -> (f64, f64) {
    let x = if alignment.ends_with("left") {
        0.0
    } else if alignment.ends_with("right") {
        1.0
    } else {
        0.5
    };

    let y = if alignment.starts_with("top") {
        0.0
    } else if alignment.starts_with("bottom") {
        1.0
    } else {
        0.5
    };

    (x, y)
}

fn aligned_offset(extra: i32, factor: f64) -> i32 {
    ((extra.max(0) as f64) * factor).round() as i32
}

fn compute_image_placement(
    object_fit: &str,
    alignment: &str,
    src_w: i32,
    src_h: i32,
    dest_x: i32,
    dest_y: i32,
    dest_w: i32,
    dest_h: i32,
    cell_width_mm: f64,
    cell_height_mm: f64,
) -> (i32, i32, i32, i32, i32, i32, i32, i32) {
    let img_aspect = src_w as f64 / src_h as f64;
    let cell_aspect = cell_width_mm / cell_height_mm;
    let (align_x, align_y) = alignment_factors(alignment);

    if object_fit == "contain" {
        // Contain: show entire image, adjust destination to maintain aspect.
        if img_aspect > cell_aspect {
            // Image wider than cell -> shrink destination height.
            let adj_h = (dest_w as f64 / img_aspect).round() as i32;
            let oy = aligned_offset(dest_h - adj_h, align_y);
            (0, 0, src_w, src_h, dest_x, dest_y + oy, dest_w, adj_h)
        } else {
            // Image taller than cell -> shrink destination width.
            let adj_w = (dest_h as f64 * img_aspect).round() as i32;
            let ox = aligned_offset(dest_w - adj_w, align_x);
            (0, 0, src_w, src_h, dest_x + ox, dest_y, adj_w, dest_h)
        }
    } else {
        // Cover: fill cell, crop excess from source according to alignment.
        if img_aspect > cell_aspect {
            // Image wider -> crop width from source.
            let vis_w = (src_h as f64 * cell_aspect).round() as i32;
            let cx = aligned_offset(src_w - vis_w, align_x);
            (cx, 0, vis_w, src_h, dest_x, dest_y, dest_w, dest_h)
        } else {
            // Image taller -> crop height from source.
            let vis_h = (src_w as f64 / cell_aspect).round() as i32;
            let cy = aligned_offset(src_h - vis_h, align_y);
            (0, cy, src_w, vis_h, dest_x, dest_y, dest_w, dest_h)
        }
    }
}

fn get_printer_name(printer_name: &str) -> Result<String, String> {
    unsafe {
        if !printer_name.is_empty() {
            return Ok(printer_name.to_string());
        }

        let mut default_name_buf = vec![0u8; 512];
        let mut default_size = default_name_buf.len() as u32;
        if GetDefaultPrinterA(Some(PSTR(default_name_buf.as_mut_ptr())), &mut default_size).0 == 0 {
            return Err("No default printer found".to_string());
        }

        let end = default_name_buf.iter().position(|&b| b == 0).unwrap_or(0);
        Ok(String::from_utf8_lossy(&default_name_buf[..end]).to_string())
    }
}

fn create_devnames(printer_name: &str) -> Result<HGLOBAL, String> {
    unsafe {
        let driver_name = b"winspool\0";
        let device_name = CString::new(printer_name)
            .map_err(|e| format!("Invalid printer name: {}", e))?
            .into_bytes_with_nul();
        let output_name = b"\0";

        let header_size = std::mem::size_of::<DEVNAMES>();
        let total_size = header_size + driver_name.len() + device_name.len() + output_name.len();
        let hdevnames = GlobalAlloc(GMEM_MOVEABLE, total_size)
            .map_err(|e| format!("Unable to allocate printer names: {}", e))?;
        let ptr = GlobalLock(hdevnames) as *mut u8;
        if ptr.is_null() {
            let _ = GlobalFree(Some(hdevnames));
            return Err("Unable to lock printer names".to_string());
        }

        let header = ptr as *mut DEVNAMES;
        (*header).wDriverOffset = header_size as u16;
        (*header).wDeviceOffset = (header_size + driver_name.len()) as u16;
        (*header).wOutputOffset = (header_size + driver_name.len() + device_name.len()) as u16;
        (*header).wDefault = 0;

        std::ptr::copy_nonoverlapping(
            driver_name.as_ptr(),
            ptr.add(header_size),
            driver_name.len(),
        );
        std::ptr::copy_nonoverlapping(
            device_name.as_ptr(),
            ptr.add(header_size + driver_name.len()),
            device_name.len(),
        );
        std::ptr::copy_nonoverlapping(
            output_name.as_ptr(),
            ptr.add(header_size + driver_name.len() + device_name.len()),
            output_name.len(),
        );

        let _ = GlobalUnlock(hdevnames);
        Ok(hdevnames)
    }
}

fn create_custom_page_devmode(grid: &GridConfig, printer_name: &str) -> Result<HGLOBAL, String> {
    unsafe {
        let c_name =
            CString::new(printer_name).map_err(|e| format!("Invalid printer name: {}", e))?;
        let printer_pcstr = PCSTR(c_name.as_ptr() as *const u8);

        let mut h_printer = PRINTER_HANDLE::default();
        if OpenPrinterA(printer_pcstr, &mut h_printer, None).is_err() {
            return Err("Unable to open printer for settings".to_string());
        }

        let dm_size = DocumentPropertiesA(None, h_printer, printer_pcstr, None, None, 0);
        if dm_size <= 0 {
            let _ = ClosePrinter(h_printer);
            return Err("Unable to read default printer settings".to_string());
        }

        let hdevmode = GlobalAlloc(GMEM_MOVEABLE, dm_size as usize)
            .map_err(|e| format!("Unable to allocate printer settings: {}", e))?;
        let devmode_ptr = GlobalLock(hdevmode) as *mut DEVMODEA;
        if devmode_ptr.is_null() {
            let _ = GlobalFree(Some(hdevmode));
            let _ = ClosePrinter(h_printer);
            return Err("Unable to lock printer settings".to_string());
        }

        let got = DocumentPropertiesA(
            None,
            h_printer,
            printer_pcstr,
            Some(devmode_ptr),
            None,
            2, // DM_OUT_BUFFER
        );

        if got < 0 {
            let _ = GlobalUnlock(hdevmode);
            let _ = GlobalFree(Some(hdevmode));
            let _ = ClosePrinter(h_printer);
            return Err("Unable to load default printer settings".to_string());
        }

        let original_icm_method = (*devmode_ptr).dmICMMethod;
        let original_icm_intent = (*devmode_ptr).dmICMIntent;

        (*devmode_ptr).dmFields |= DM_PAPERSIZE
            | DM_PAPERLENGTH
            | DM_PAPERWIDTH
            | DM_ORIENTATION
            | DM_SCALE
            | DM_COLOR
            | DM_ICMMETHOD
            | DM_ICMINTENT;

        // Match standard paper sizes to help modern dialogs recognize them
        let w_mm = grid.page_width.round() as i32;
        let h_mm = grid.page_height.round() as i32;

        if (w_mm == 210 && h_mm == 297) || (w_mm == 297 && h_mm == 210) {
            (*devmode_ptr).Anonymous1.Anonymous1.dmPaperSize = 9; // DMPAPER_A4
        } else if (w_mm == 216 && h_mm == 279) || (w_mm == 279 && h_mm == 216) {
            (*devmode_ptr).Anonymous1.Anonymous1.dmPaperSize = 1; // DMPAPER_LETTER
        } else {
            (*devmode_ptr).Anonymous1.Anonymous1.dmPaperSize = 0; // Custom
        }

        (*devmode_ptr).Anonymous1.Anonymous1.dmPaperWidth = (grid.page_width * 10.0).round() as i16;
        (*devmode_ptr).Anonymous1.Anonymous1.dmPaperLength =
            (grid.page_height * 10.0).round() as i16;
        // Set orientation based on dimensions
        if grid.page_width > grid.page_height {
            (*devmode_ptr).Anonymous1.Anonymous1.dmOrientation = 2; // DMORIENT_LANDSCAPE
        } else {
            (*devmode_ptr).Anonymous1.Anonymous1.dmOrientation = 1; // DMORIENT_PORTRAIT
        }
        (*devmode_ptr).Anonymous1.Anonymous1.dmScale = 100;

        // Match old photo apps more closely: do not do app-side color correction,
        // but ask the printer driver to own photographic color handling.
        (*devmode_ptr).dmColor = DMCOLOR_COLOR;
        (*devmode_ptr).dmICMMethod = DMICMMETHOD_DRIVER;
        (*devmode_ptr).dmICMIntent = DMICM_CONTRAST;

        // --- CRITICAL: Let the driver validate and merge the changes ---
        // This ensures the driver recognizes the custom paper size/orientation
        // and updates internal private fields if necessary.
        let mut validated = DocumentPropertiesA(
            None,
            h_printer,
            printer_pcstr,
            Some(devmode_ptr),
            Some(devmode_ptr),
            (DM_IN_BUFFER.0 | DM_OUT_BUFFER.0) as u32,
        );

        if validated < 0 {
            (*devmode_ptr).dmICMMethod = DMICMMETHOD_NONE;
            validated = DocumentPropertiesA(
                None,
                h_printer,
                printer_pcstr,
                Some(devmode_ptr),
                Some(devmode_ptr),
                (DM_IN_BUFFER.0 | DM_OUT_BUFFER.0) as u32,
            );
        }

        if validated < 0 {
            (*devmode_ptr).dmFields &= !(DM_ICMMETHOD | DM_ICMINTENT);
            (*devmode_ptr).dmICMMethod = original_icm_method;
            (*devmode_ptr).dmICMIntent = original_icm_intent;
            validated = DocumentPropertiesA(
                None,
                h_printer,
                printer_pcstr,
                Some(devmode_ptr),
                Some(devmode_ptr),
                (DM_IN_BUFFER.0 | DM_OUT_BUFFER.0) as u32,
            );
        }

        let _ = ClosePrinter(h_printer);
        if validated < 0 {
            let _ = GlobalUnlock(hdevmode);
            let _ = GlobalFree(Some(hdevmode));
            return Err("Unable to validate printer settings".to_string());
        }

        let _ = GlobalUnlock(hdevmode);
        Ok(hdevmode)
    }
}
fn show_print_dialog(
    grid: &GridConfig,
    printer_name: &str,
    _hwnd: Option<usize>,
) -> Result<HDC, String> {
    unsafe {
        let resolved_printer = get_printer_name(printer_name)?;
        let hdevmode = create_custom_page_devmode(grid, &resolved_printer)?;
        let hdevnames = create_devnames(&resolved_printer)?;
        let mut dialog = PRINTDLGA {
            lStructSize: std::mem::size_of::<PRINTDLGA>() as u32,
            hwndOwner: windows::Win32::Foundation::HWND::default(), // Setting to NULL forces legacy dialog on Win11
            hDevMode: hdevmode,
            hDevNames: hdevnames,
            Flags: PD_RETURNDC | PD_NOSELECTION | PD_NOPAGENUMS | PD_HIDEPRINTTOFILE,
            nMinPage: 1,
            nMaxPage: 1,
            nFromPage: 1,
            nToPage: 1,
            nCopies: 1,
            ..Default::default()
        };

        if PrintDlgA(&mut dialog).as_bool() {
            if !dialog.hDevMode.is_invalid() {
                let _ = GlobalFree(Some(dialog.hDevMode));
            }
            if !dialog.hDevNames.is_invalid() {
                let _ = GlobalFree(Some(dialog.hDevNames));
            }

            if dialog.hDC.is_invalid() {
                return Err("Print dialog did not return a printer device context".to_string());
            }

            return Ok(dialog.hDC);
        }

        let err = CommDlgExtendedError().0;
        if !dialog.hDevMode.is_invalid() {
            let _ = GlobalFree(Some(dialog.hDevMode));
        }
        if !dialog.hDevNames.is_invalid() {
            let _ = GlobalFree(Some(dialog.hDevNames));
        }

        if err == 0 {
            Err("Print cancelled".to_string())
        } else {
            Err(format!("Print dialog failed with error {}", err))
        }
    }
}

/// Send the print job directly to a printer via Win32 GDI.
pub fn print_job(job: &PrintJob, printer_name: &str, hwnd: Option<usize>) -> Result<(), String> {
    let grid = &job.grid;

    unsafe {
        // --- 1. Get printer DC ---
        let hdc = show_print_dialog(grid, printer_name, hwnd)?;

        if hdc.is_invalid() {
            return Err("Failed to create printer device context".to_string());
        }

        // --- 2. Start print job ---
        let doc_name = CString::new("PrintCatalog Photo Grid").unwrap();
        let doc_info = DOCINFOA {
            cbSize: std::mem::size_of::<DOCINFOA>() as i32,
            lpszDocName: PCSTR(doc_name.as_ptr() as *const u8),
            lpszOutput: PCSTR::null(),
            lpszDatatype: PCSTR::null(),
            fwType: 0,
        };

        if StartDocA(hdc, &doc_info) <= 0 {
            let _ = DeleteDC(hdc);
            return Err("Failed to start print job".to_string());
        }

        if StartPage(hdc) <= 0 {
            EndDoc(hdc);
            let _ = DeleteDC(hdc);
            return Err("Failed to start page".to_string());
        }

        // High-quality scaling mode
        SetStretchBltMode(hdc, HALFTONE);
        let _ = SetBrushOrgEx(hdc, 0, 0, None);

        // --- 3. Printer physical dimensions ---
        let log_pixels_x = GetDeviceCaps(Some(hdc), LOGPIXELSX);
        let log_pixels_y = GetDeviceCaps(Some(hdc), LOGPIXELSY);
        let scale_x = log_pixels_x as f64 / 25.4;
        let scale_y = log_pixels_y as f64 / 25.4;

        println!(
            "[Printer] Page: {}x{} mm → DPI: {}x{} ({:.1}x{:.1} px/mm)",
            grid.page_width, grid.page_height, log_pixels_x, log_pixels_y, scale_x, scale_y
        );

        // --- 4. Draw each cell, caching decoded images without changing draw order ---
        struct CachedImage {
            src_w: i32,
            src_h: i32,
            bgr: Vec<u8>,
            bmi: BITMAPINFO,
        }

        let mut image_cache: std::collections::HashMap<(String, i32), CachedImage> =
            std::collections::HashMap::new();

        for cell in &job.cells {
            if cell.row >= grid.rows || cell.col >= grid.cols {
                println!(
                    "[Printer] Skipping out-of-grid cell ({},{}) for {}x{} grid",
                    cell.row, cell.col, grid.rows, grid.cols
                );
                continue;
            }

            let cache_key = (cell.image_id.clone(), cell.rotation);

            if !image_cache.contains_key(&cache_key) {
                let image_data = job
                    .images
                    .get(&cell.image_id)
                    .ok_or_else(|| format!("Image ID {} not found in job images", cell.image_id))?;

                let img = crate::print_engine::prepare_cell_image(image_data, cell.rotation)?;
                let rgba = img.to_rgba8();
                let src_w = rgba.width() as i32;
                let src_h = rgba.height() as i32;
                let bgr = rgba_to_bgr(&rgba);

                let bmi = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: src_w,
                        biHeight: src_h,
                        biPlanes: 1,
                        biBitCount: 24,
                        biCompression: 0,
                        biSizeImage: bgr.len() as u32,
                        biXPelsPerMeter: 0,
                        biYPelsPerMeter: 0,
                        biClrUsed: 0,
                        biClrImportant: 0,
                    },
                    bmiColors: [RGBQUAD::default()],
                };

                image_cache.insert(
                    cache_key.clone(),
                    CachedImage {
                        src_w,
                        src_h,
                        bgr,
                        bmi,
                    },
                );
            }

            let cached = image_cache
                .get(&cache_key)
                .ok_or_else(|| "Image cache lookup failed".to_string())?;

            // Destination rectangle in printer device units
            let cell_x_mm = grid.padding_left + cell.col as f64 * (grid.cell_width + grid.gap_x);
            let cell_y_mm = grid.padding_top + cell.row as f64 * (grid.cell_height + grid.gap_y);
            let dest_x = (cell_x_mm * scale_x).round() as i32;
            let dest_y = (cell_y_mm * scale_y).round() as i32;
            let dest_w = (grid.cell_width * scale_x).round() as i32;
            let dest_h = (grid.cell_height * scale_y).round() as i32;

            let (sx, sy, sw, sh, dx, dy, dw, dh) = compute_image_placement(
                &cell.object_fit,
                &cell.alignment,
                cached.src_w,
                cached.src_h,
                dest_x,
                dest_y,
                dest_w,
                dest_h,
                grid.cell_width,
                grid.cell_height,
            );

            println!(
                "[Printer] Cell ({},{}) — {}x{}px → src({},{} {}x{}) → dest({},{} {}x{})",
                cell.row, cell.col, cached.src_w, cached.src_h, sx, sy, sw, sh, dx, dy, dw, dh
            );

            let result = StretchDIBits(
                hdc,
                dx,
                dy,
                dw,
                dh,
                sx,
                sy,
                sw,
                sh,
                Some(cached.bgr.as_ptr() as *const _),
                &cached.bmi,
                DIB_RGB_COLORS,
                SRCCOPY,
            );

            if result == 0 {
                EndPage(hdc);
                EndDoc(hdc);
                let _ = DeleteDC(hdc);
                return Err("StretchDIBits failed".to_string());
            }

            // Vector outline using GDI pen (crisp at any DPI)
            if cell.outline {
                // ~0.13mm pen width (matches professional photo apps)
                let pen_w = (0.13 * scale_x).round().max(1.0) as i32;
                let pen = CreatePen(PS_SOLID, pen_w, COLORREF(0x00000000));
                let old_pen = SelectObject(hdc, pen.into());
                let old_brush = SelectObject(hdc, GetStockObject(NULL_BRUSH));
                let _ = Rectangle(hdc, dest_x, dest_y, dest_x + dest_w, dest_y + dest_h);
                SelectObject(hdc, old_pen);
                SelectObject(hdc, old_brush);
                let _ = DeleteObject(pen.into());
            }
        }

        println!("[Printer] All cells sent successfully");

        // --- 5. Finish ---
        EndPage(hdc);
        EndDoc(hdc);
        let _ = DeleteDC(hdc);

        println!("[Printer] Print job completed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::compute_image_placement;

    #[test]
    fn cover_alignment_crops_wide_source_from_requested_side() {
        let left = compute_image_placement(
            "cover",
            "center-left",
            400,
            200,
            10,
            20,
            100,
            100,
            50.0,
            50.0,
        );
        let right = compute_image_placement(
            "cover",
            "center-right",
            400,
            200,
            10,
            20,
            100,
            100,
            50.0,
            50.0,
        );

        assert_eq!(left, (0, 0, 200, 200, 10, 20, 100, 100));
        assert_eq!(right, (200, 0, 200, 200, 10, 20, 100, 100));
    }

    #[test]
    fn contain_alignment_places_narrow_source_in_requested_position() {
        let bottom_right = compute_image_placement(
            "contain",
            "bottom-right",
            100,
            200,
            10,
            20,
            100,
            100,
            50.0,
            50.0,
        );

        assert_eq!(bottom_right, (0, 0, 100, 200, 60, 20, 50, 100));
    }

    #[test]
    fn unknown_alignment_defaults_to_center() {
        let centered = compute_image_placement(
            "contain",
            "unexpected",
            400,
            200,
            10,
            20,
            100,
            100,
            50.0,
            50.0,
        );

        assert_eq!(centered, (0, 0, 400, 200, 10, 45, 100, 50));
    }
}
