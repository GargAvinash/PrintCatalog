# GEMINI.md

This project, **PrintCatalog**, is a professional-grade desktop application designed for arranging photos in precise grid layouts (e.g., passport photos, ID cards) and printing them at full resolution by bypassing the browser's print pipeline and using a native Win32 GDI printing engine.

## Project Overview

-   **Purpose:** High-fidelity photo grid printing directly to system printers, prioritizing maximum image quality and speed.
-   **Core Philosophy:** The primary goal is to print high-quality photos without any software-side quality compromise or color modification. We must send the raw pixel data directly to the printer driver. This allows the printer's own firmware and driver settings to handle color management and customization, mirroring the behavior of professional tools like **NewSoft Presto! Mr. Photo**.
-   **Architecture:** 
    -   **Frontend:** React 19 SPA built with Vite and TailwindCSS 4. It handles the UI for grid configuration, photo cataloging, and layout arrangement.
    -   **Backend (Tauri v2):** Rust-based backend that manages system integration, image decoding/rotation, and direct communication with Windows printing APIs.
-   **Communication:** IPC via Tauri commands. Payload sizes are strictly optimized to ensure the application remains fast and responsive even with 10MB+ images.

## Building and Running

### Prerequisites
- Node.js (v18+)
- Rust (stable)
- Windows 10/11 (Required for Win32 Printing APIs)
- Visual Studio Build Tools with "Desktop development with C++"

### Key Commands
-   **Development Mode:** `npm run tauri dev`
    -   Starts the Vite dev server and the Tauri desktop application.
-   **Build Production:** `npm run tauri build`
    -   **CRITICAL:** Never execute the build command autonomously. Always ask the user to run it, as it is a time-intensive process.
-   **Frontend Only Dev:** `npm run dev`

## Development Conventions

-   **Testing & Committing Mandate:** NEVER perform a `git commit` or finalize a task without first asking the user to manually test the application by running `npm run tauri dev`. You are encouraged to run `cargo check`, `cargo test`, or frontend linting/type-checks to verify compilation and basic logic before asking for manual validation.
-   **High-Quality Printing Mandate:** Raw pixel data must be passed to the printer via Win32 GDI (`StretchDIBits` with `HALFTONE`) without applying intermediate color profiles or compression. The printer driver must receive the data as "raw" as possible.
-   **Performance Goal:** The application must aim for the speed and accuracy of **Presto! Mr. Photo**, handling high-resolution assets instantly.
-   **IPC Optimization:** Never send redundant image data over the bridge. Use unique IDs and a central image map.
-   **Image Processing:** Rotation is the only transformation allowed on the raw data before printing. No resizing or color correction.
-   **Validation:** When modifying the print pipeline, ensure that the output matches the original file's fidelity.

## Project Structure

-   `src/`: React frontend source code.
    -   `App.tsx`: Main application component and state logic.
-   `src-tauri/`: Rust backend source code.
    -   `src/lib.rs`: Tauri command definitions and application entry point.
    -   `src/printer.rs`: Core Win32 GDI printing implementation.
    -   `src/print_engine.rs`: Image decoding and rotation logic.
-   `website/`: Static website assets for the project.
