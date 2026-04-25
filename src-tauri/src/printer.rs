/// Win32 Printer Integration — Direct OS printing via GDI.
///
/// This module uses the Windows GDI printing API to send the composed
/// high-resolution image directly to the printer, completely bypassing
/// any browser or application print dialog.
///
/// The flow: composed RGBA image → BMP in memory → StretchDIBits to printer DC

use image::RgbaImage;
use std::ffi::CString;


use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::Printing::*;
use windows::Win32::Storage::Xps::*;
use windows::core::{PCSTR, PSTR};

use crate::{GridConfig, PrinterInfo};

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

/// Send a composed RGBA image directly to a printer via Win32 GDI.
///
/// This uses StretchDIBits to send the image at full resolution to the
/// printer's device context. The printer driver handles final DPI mapping.
pub fn print_image(
    image: &RgbaImage,
    grid: &GridConfig,
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
            DeleteDC(hdc);
            return Err("Failed to start print job".to_string());
        }

        if StartPage(hdc) <= 0 {
            EndDoc(hdc);
            DeleteDC(hdc);
            return Err("Failed to start page".to_string());
        }

        // --- 3. Get printer's physical dimensions ---
        let printer_w = GetDeviceCaps(Some(hdc), HORZRES);
        let printer_h = GetDeviceCaps(Some(hdc), VERTRES);

        println!(
            "[Printer] Printer resolution: {}x{} device units",
            printer_w, printer_h
        );

        // --- 4. Prepare bitmap info ---
        let width = image.width() as i32;
        let height = image.height() as i32;

        // Convert RGBA to BGR (GDI expects BGR bottom-up)
        let mut bgr_pixels: Vec<u8> = Vec::with_capacity((width * height * 3) as usize);
        // GDI bitmaps are bottom-up, so iterate rows in reverse
        for y in (0..height).rev() {
            for x in 0..width {
                let pixel = image.get_pixel(x as u32, y as u32);
                bgr_pixels.push(pixel[2]); // B
                bgr_pixels.push(pixel[1]); // G
                bgr_pixels.push(pixel[0]); // R
            }
            // Pad rows to 4-byte alignment
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
                biHeight: height, // positive = bottom-up
                biPlanes: 1,
                biBitCount: 24,
                biCompression: 0, // BI_RGB
                biSizeImage: bgr_pixels.len() as u32,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [RGBQUAD::default()],
        };

        // --- 5. Send image to printer ---
        // StretchDIBits scales our image to fill the printer's page area
        let result = StretchDIBits(
            hdc,
            0,              // dest X
            0,              // dest Y
            printer_w,      // dest width (full printer page)
            printer_h,      // dest height (full printer page)
            0,              // src X
            0,              // src Y
            width,          // src width
            height,         // src height
            Some(bgr_pixels.as_ptr() as *const _),
            &bmi,
            DIB_RGB_COLORS,
            SRCCOPY,
        );

        if result == 0 {
            EndPage(hdc);
            EndDoc(hdc);
            DeleteDC(hdc);
            return Err("StretchDIBits failed".to_string());
        }

        println!("[Printer] Image sent to printer successfully");

        // --- 6. Finish ---
        EndPage(hdc);
        EndDoc(hdc);
        DeleteDC(hdc);

        println!("[Printer] Print job completed");
        Ok(())
    }
}
