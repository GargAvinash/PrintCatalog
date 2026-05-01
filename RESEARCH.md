# Research and Development Notes

## Windows Print Dialog Research: Legacy vs. Modern (Windows 11)
*Investigation into paper size overrides and dialog behavior.*

### The Problem
When the application triggered the modern Windows 11 print dialog (Unified Print Dialog), it would frequently ignore the `DEVMODE` settings (specifically `dmPaperSize`, `dmPaperWidth`, and `dmPaperLength`) passed by the application. Instead, it would default to the user's last-used printer settings or the printer's system default.

### Key Findings

#### 1. Unified Print Dialog Quirks
- **Behavior:** Introduced in Windows 11 22H2, the modern dialog acts as a UWP-style wrapper. It is known to override binary `DEVMODE` buffers with its own cached preferences upon initialization.
- **Triggers:** Windows 11 typically triggers the modern dialog if a valid `hwndOwner` (window handle) is passed to `PrintDlgA/W` or if the newer `PrintDlgEx` API is used.

#### 2. The Legacy Fallback (Our Solution)
- **The Fix:** By setting `hwndOwner` to `NULL` (or `HWND(0)`) in the `PRINTDLGA` structure, Windows 11 falls back to the classic Win32 print dialog (`comdlg32.dll`).
- **Result:** The legacy dialog reliably respects the `DEVMODE` buffer, allowing the application to automatically set the paper size to A4, Letter, or Custom based on the current grid template.
- **Responsiveness:** Since the printing command is handled in a Tauri background thread (`spawn_blocking`), using a "standalone" legacy dialog does not hang the main application UI.

#### 3. Technical Opinion
For a professional-grade printing tool like **PrintCatalog**, the **Legacy Dialog is superior**:
- **Direct Driver Access:** It provides a direct "Properties" button that opens the printer manufacturer's original driver interface. This is essential for advanced features like high-DPI settings, specialized media types, and color management.
- **Fidelity:** It does not attempt to "simplify" or re-interpret the print job, ensuring the raw pixel data and GDI commands reach the driver exactly as intended.
- **Predictability:** It avoids the "Unified" dialog's tendency to silently change paper sizes or orientations based on previous unrelated print jobs.

### Future Considerations
To support the modern dialog in the future without losing settings, the project would likely need to implement **Print Schema (XML)** and `IPrintTicket` interfaces. Given the requirement to access advanced printer properties, the legacy dialog remains the recommended path.
