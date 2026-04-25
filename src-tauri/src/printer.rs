/// Win32 Printer Integration — Direct OS printing via GDI.
///
/// Sends original-resolution images directly to the printer.
/// The printer driver handles all DPI mapping and scaling.
/// Uses source-crop StretchDIBits for cover/contain and GDI vector outlines.

use crate::{PrintJob, PrinterInfo};
use std::ffi::CString;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::Printing::*;
use windows::Win32::Storage::Xps::*;
use windows::Win32::Foundation::COLORREF;
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
        let has_default = GetDefaultPrinterA(Some(PSTR(default_name_buf.as_mut_ptr())), &mut default_size).0 != 0;
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
    let w = img.width() as i32;
    let h = img.height() as i32;
    let row_bytes = w * 3;
    let padding = (4 - (row_bytes % 4)) % 4;

    let mut bgr = Vec::with_capacity(((row_bytes + padding) * h) as usize);
    for y in (0..h).rev() {
        for x in 0..w {
            let px = img.get_pixel(x as u32, y as u32);
            bgr.push(px[2]);
            bgr.push(px[1]);
            bgr.push(px[0]);
        }
        for _ in 0..padding {
            bgr.push(0);
        }
    }
    bgr
}

/// Send the print job directly to a printer via Win32 GDI.
pub fn print_job(job: &PrintJob, printer_name: &str) -> Result<(), String> {
    let grid = &job.grid;

    unsafe {
        // --- 1. Get printer DC ---
        let c_name = if printer_name.is_empty() {
            let mut buf = vec![0u8; 512];
            let mut size = buf.len() as u32;
            if GetDefaultPrinterA(Some(PSTR(buf.as_mut_ptr())), &mut size).0 == 0 {
                return Err("No default printer found".to_string());
            }
            let end = buf.iter().position(|&b| b == 0).unwrap_or(0);
            CString::new(&buf[..end]).map_err(|e| format!("Invalid printer name: {}", e))?
        } else {
            CString::new(printer_name).map_err(|e| format!("Invalid printer name: {}", e))?
        };

        let printer_pcstr = PCSTR(c_name.as_ptr() as *const u8);

        // --- Set custom paper size via DEVMODE ---
        // DEVMODEA has complex unions in the windows crate, so we manipulate
        // the raw bytes at the well-defined Microsoft ABI offsets:
        //   offset 40: dmFields (u32)
        //   offset 46: dmPaperSize (i16)
        //   offset 48: dmPaperLength (i16)  — in tenths of mm
        //   offset 50: dmPaperWidth (i16)   — in tenths of mm
        const DEVMODE_OFFSET_FIELDS: usize = 40;
        const DEVMODE_OFFSET_PAPER_SIZE: usize = 46;
        const DEVMODE_OFFSET_PAPER_LENGTH: usize = 48;
        const DEVMODE_OFFSET_PAPER_WIDTH: usize = 50;
        const DM_PAPERSIZE: u32 = 0x0002;
        const DM_PAPERLENGTH: u32 = 0x0004;
        const DM_PAPERWIDTH: u32 = 0x0008;

        // Query DEVMODE buffer size
        let dm_size = DocumentPropertiesA(
            None, PRINTER_HANDLE::default(), printer_pcstr, None, None, 0,
        );

        let hdc = if dm_size > 0 {
            let mut dm_buf = vec![0u8; dm_size as usize];

            // Fill with printer defaults
            let got = DocumentPropertiesA(
                None, PRINTER_HANDLE::default(), printer_pcstr,
                Some(dm_buf.as_mut_ptr() as *mut _),
                None, 2, // DM_OUT_BUFFER
            );

            if got >= 0 && dm_buf.len() >= 52 {
                // Set custom paper: DMPAPER_USER = 0 (custom size)
                let paper_w_tenths = (grid.page_width * 10.0).round() as i16;
                let paper_h_tenths = (grid.page_height * 10.0).round() as i16;

                // Update dmFields
                let fields_ptr = dm_buf.as_mut_ptr().add(DEVMODE_OFFSET_FIELDS) as *mut u32;
                *fields_ptr |= DM_PAPERSIZE | DM_PAPERLENGTH | DM_PAPERWIDTH;

                // dmPaperSize = 0 (custom)
                let ps_ptr = dm_buf.as_mut_ptr().add(DEVMODE_OFFSET_PAPER_SIZE) as *mut i16;
                *ps_ptr = 0;

                // dmPaperLength = height in tenths of mm
                let pl_ptr = dm_buf.as_mut_ptr().add(DEVMODE_OFFSET_PAPER_LENGTH) as *mut i16;
                *pl_ptr = paper_h_tenths;

                // dmPaperWidth = width in tenths of mm
                let pw_ptr = dm_buf.as_mut_ptr().add(DEVMODE_OFFSET_PAPER_WIDTH) as *mut i16;
                *pw_ptr = paper_w_tenths;

                println!(
                    "[Printer] DEVMODE: custom paper {}x{} tenths-mm",
                    paper_w_tenths, paper_h_tenths
                );

                CreateDCA(
                    PCSTR::null(), printer_pcstr, PCSTR::null(),
                    Some(dm_buf.as_ptr() as *const _),
                )
            } else {
                println!("[Printer] Warning: DEVMODE query failed, using printer defaults");
                CreateDCA(PCSTR::null(), printer_pcstr, PCSTR::null(), None)
            }
        } else {
            println!("[Printer] Warning: DocumentProperties unavailable, using defaults");
            CreateDCA(PCSTR::null(), printer_pcstr, PCSTR::null(), None)
        };

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

        // --- 3. Printer physical dimensions ---
        let printer_w = GetDeviceCaps(Some(hdc), HORZRES);
        let printer_h = GetDeviceCaps(Some(hdc), VERTRES);
        let scale_x = printer_w as f64 / grid.page_width;
        let scale_y = printer_h as f64 / grid.page_height;

        println!(
            "[Printer] Page: {}x{} mm → {}x{} device units ({:.1}x{:.1} px/mm)",
            grid.page_width, grid.page_height, printer_w, printer_h, scale_x, scale_y
        );

        // --- 4. Draw each cell ---
        for cell in &job.cells {
            let img = crate::print_engine::prepare_cell_image(cell)?;
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

            // Destination rectangle in printer device units
            let cell_x_mm = grid.padding_left + cell.col as f64 * (grid.cell_width + grid.gap_x);
            let cell_y_mm = grid.padding_top + cell.row as f64 * (grid.cell_height + grid.gap_y);
            let dest_x = (cell_x_mm * scale_x).round() as i32;
            let dest_y = (cell_y_mm * scale_y).round() as i32;
            let dest_w = (grid.cell_width * scale_x).round() as i32;
            let dest_h = (grid.cell_height * scale_y).round() as i32;

            // Source crop for cover/contain — crop from source, not destination
            let img_aspect = src_w as f64 / src_h as f64;
            let cell_aspect = grid.cell_width / grid.cell_height;

            let (sx, sy, sw, sh, dx, dy, dw, dh) = if cell.object_fit == "contain" {
                // Contain: show entire image, adjust destination to maintain aspect
                if img_aspect > cell_aspect {
                    // Image wider than cell → shrink dest height
                    let adj_h = (dest_w as f64 / img_aspect).round() as i32;
                    let oy = (dest_h - adj_h) / 2;
                    (0, 0, src_w, src_h, dest_x, dest_y + oy, dest_w, adj_h)
                } else {
                    // Image taller than cell → shrink dest width
                    let adj_w = (dest_h as f64 * img_aspect).round() as i32;
                    let ox = (dest_w - adj_w) / 2;
                    (0, 0, src_w, src_h, dest_x + ox, dest_y, adj_w, dest_h)
                }
            } else {
                // Cover: fill cell, crop excess from source
                if img_aspect > cell_aspect {
                    // Image wider → crop width from source
                    let vis_w = (src_h as f64 * cell_aspect).round() as i32;
                    let cx = (src_w - vis_w) / 2;
                    (cx, 0, vis_w, src_h, dest_x, dest_y, dest_w, dest_h)
                } else {
                    // Image taller → crop height from source
                    let vis_h = (src_w as f64 / cell_aspect).round() as i32;
                    let cy = (src_h - vis_h) / 2;
                    (0, cy, src_w, vis_h, dest_x, dest_y, dest_w, dest_h)
                }
            };

            println!(
                "[Printer] Cell ({},{}) — {}x{}px → src({},{} {}x{}) → dest({},{} {}x{})",
                cell.row, cell.col, src_w, src_h, sx, sy, sw, sh, dx, dy, dw, dh
            );

            let result = StretchDIBits(
                hdc, dx, dy, dw, dh, sx, sy, sw, sh,
                Some(bgr.as_ptr() as *const _),
                &bmi, DIB_RGB_COLORS, SRCCOPY,
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
                Rectangle(hdc, dest_x, dest_y, dest_x + dest_w, dest_y + dest_h);
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
