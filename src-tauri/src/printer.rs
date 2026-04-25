/// Win32 Printer Integration — Direct OS printing via GDI.
///
/// This module uses the Windows GDI printing API to send the composed
/// high-resolution image directly to the printer, completely bypassing
/// any browser or application print dialog.
///
/// The flow: composed RGBA image → BMP in memory → StretchDIBits to printer DC
use crate::{PrintJob, PrinterInfo};
use std::ffi::CString;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::Printing::*;
use windows::Win32::Storage::Xps::*;
use windows::core::{PCSTR, PSTR};

/// List all available printers on the system
pub fn list_printers() -> Result<Vec<PrinterInfo>, String> {
    // Use EnumPrinters to get local + network printers
    unsafe {
        let mut bytes_needed: u32 = 0;
        let mut count: u32 = 0;

        // First call to get buffer size
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

        // Get default printer name
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

/// Send the print job directly to a printer via Win32 GDI.
///
/// This iterates over each cell, fetches the native image data, and uses
/// StretchDIBits to send each image directly to the printer's device context.
/// The printer driver handles the final high-quality DPI mapping and scaling.
pub fn print_job(
    job: &PrintJob,
    printer_name: &str,
) -> Result<(), String> {
    unsafe {
        // --- 1. Get printer device context ---
        let hdc = if printer_name.is_empty() {
            // Use default printer
            let mut default_name_buf = vec![0u8; 512];
            let mut size = default_name_buf.len() as u32;
            if GetDefaultPrinterA(Some(PSTR(default_name_buf.as_mut_ptr())), &mut size).0 == 0 {
                return Err("No default printer found".to_string());
            }

            let end = default_name_buf.iter().position(|&b| b == 0).unwrap_or(0);
            let default_name = CString::new(&default_name_buf[..end])
                .map_err(|e| format!("Invalid printer name: {}", e))?;

            CreateDCA(PCSTR::null(), PCSTR(default_name.as_ptr() as *const u8), PCSTR::null(), None)
        } else {
            let c_name = CString::new(printer_name)
                .map_err(|e| format!("Invalid printer name: {}", e))?;
            CreateDCA(PCSTR::null(), PCSTR(c_name.as_ptr() as *const u8), PCSTR::null(), None)
        };

        if hdc.is_invalid() {
            return Err("Failed to create printer device context".to_string());
        }

        println!("[Printer] Got printer DC, starting print job...");

        // --- 2. Start print job ---
        let doc_name = CString::new("PrintCatalog Photo Grid").unwrap();
        let doc_info = DOCINFOA {
            cbSize: std::mem::size_of::<DOCINFOA>() as i32,
            lpszDocName: PCSTR(doc_name.as_ptr() as *const u8),
            lpszOutput: PCSTR::null(),
            lpszDatatype: PCSTR::null(),
            fwType: 0,
        };

        let job_id = StartDocA(hdc, &doc_info);
        if job_id <= 0 {
            let _ = DeleteDC(hdc);
            return Err("Failed to start print job".to_string());
        }

        if StartPage(hdc) <= 0 {
            let _ = EndDoc(hdc);
            let _ = DeleteDC(hdc);
            return Err("Failed to start page".to_string());
        }

        // Enable high-quality resampling in GDI
        SetStretchBltMode(hdc, HALFTONE);

        // --- 3. Get printer's physical dimensions ---
        let printer_w = GetDeviceCaps(Some(hdc), HORZRES);
        let printer_h = GetDeviceCaps(Some(hdc), VERTRES);

        let grid = &job.grid;
        // Scale UI mm to Printer device pixels
        let scale_x = printer_w as f64 / grid.page_width;
        let scale_y = printer_h as f64 / grid.page_height;

        println!(
            "[Printer] Printer resolution: {}x{} device units. Scale: {:.2} px/mm",
            printer_w, printer_h, scale_x
        );

        // --- 4. Process and draw each cell natively ---
        for cell in &job.cells {
            let processed_img = crate::print_engine::process_cell_image(
                cell,
                grid.cell_width,
                grid.cell_height
            )?;

            let width = processed_img.width() as i32;
            let height = processed_img.height() as i32;

            // Convert RGBA to BGR (GDI expects BGR bottom-up)
            let mut bgr_pixels: Vec<u8> = Vec::with_capacity((width * height * 3) as usize);
            for y in (0..height).rev() {
                for x in 0..width {
                    let pixel = processed_img.get_pixel(x as u32, y as u32);
                    bgr_pixels.push(pixel[2]); // B
                    bgr_pixels.push(pixel[1]); // G
                    bgr_pixels.push(pixel[0]); // R
                }
                let row_bytes = width * 3;
                let padding = (4 - (row_bytes % 4)) % 4;
                for _ in 0..padding {
                    bgr_pixels.push(0);
                }
            }

            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: height,
                    biPlanes: 1,
                    biBitCount: 24,
                    biCompression: 0,
                    biSizeImage: bgr_pixels.len() as u32,
                    biXPelsPerMeter: 0,
                    biYPelsPerMeter: 0,
                    biClrUsed: 0,
                    biClrImportant: 0,
                },
                bmiColors: [RGBQUAD::default()],
            };

            // Calculate destination rectangle in printer device units
            let cell_x_mm = grid.padding_left + cell.col as f64 * (grid.cell_width + grid.gap_x);
            let cell_y_mm = grid.padding_top + cell.row as f64 * (grid.cell_height + grid.gap_y);

            let dest_x = (cell_x_mm * scale_x).round() as i32;
            let dest_y = (cell_y_mm * scale_y).round() as i32;
            let dest_w = (grid.cell_width * scale_x).round() as i32;
            let dest_h = (grid.cell_height * scale_y).round() as i32;

            let result = StretchDIBits(
                hdc,
                dest_x, dest_y, dest_w, dest_h,
                0, 0, width, height,
                Some(bgr_pixels.as_ptr() as *const _),
                &bmi,
                DIB_RGB_COLORS,
                SRCCOPY,
            );

            if result == 0 {
                let _ = EndPage(hdc);
                let _ = EndDoc(hdc);
                let _ = DeleteDC(hdc);
                return Err("StretchDIBits failed on cell".to_string());
            }

            // Draw outline if requested
            if cell.outline {
                let black: [u8; 4] = [0, 0, 0, 0];
                let bmi_black = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: 1,
                        biHeight: 1,
                        biPlanes: 1,
                        biBitCount: 24,
                        biCompression: 0,
                        biSizeImage: 4,
                        biXPelsPerMeter: 0,
                        biYPelsPerMeter: 0,
                        biClrUsed: 0,
                        biClrImportant: 0,
                    },
                    bmiColors: [RGBQUAD::default()],
                };

                let thick = (2.0 * scale_x / 10.0).max(2.0) as i32; // Scale outline thickness based on DPI
                let p = Some(black.as_ptr() as *const _);
                let b = &bmi_black;

                StretchDIBits(hdc, dest_x, dest_y, dest_w, thick, 0, 0, 1, 1, p, b, DIB_RGB_COLORS, SRCCOPY); // Top
                StretchDIBits(hdc, dest_x, dest_y + dest_h - thick, dest_w, thick, 0, 0, 1, 1, p, b, DIB_RGB_COLORS, SRCCOPY); // Bottom
                StretchDIBits(hdc, dest_x, dest_y, thick, dest_h, 0, 0, 1, 1, p, b, DIB_RGB_COLORS, SRCCOPY); // Left
                StretchDIBits(hdc, dest_x + dest_w - thick, dest_y, thick, dest_h, 0, 0, 1, 1, p, b, DIB_RGB_COLORS, SRCCOPY); // Right
            }
        }

        println!("[Printer] All images sent successfully");

        // --- 5. Finish ---
        let _ = EndPage(hdc);
        let _ = EndDoc(hdc);
        let _ = DeleteDC(hdc);

        println!("[Printer] Print job completed");
        Ok(())
    }
}
