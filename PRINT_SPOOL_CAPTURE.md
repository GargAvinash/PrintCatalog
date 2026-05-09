# Print Spool Capture Without a Physical Printer

Windows can already act like a useful virtual print capture target. For GDI apps such as PrintCatalog and Mr. Photo, the most useful thing to inspect is usually the EMF spool file, because it can contain `EMR_STRETCHDIBITS` records with the bitmap payloads passed to GDI.

This does not simulate ink, paper, or the real printer driver's final color conversion. It helps answer a narrower question: did the app hand Windows the same pixels, placement, and scaling data as Mr. Photo?

## Capture a Spool File

1. Open the printer queue for the target printer or virtual printer.
2. Open printer properties and enable keeping printed documents.
3. Pause the printer queue.
4. Print one page from the app.
5. Copy the new `.spl` file from:

   `C:\Windows\System32\spool\PRINTERS`

   This usually requires administrator access.
6. Resume or clear the queue when done.

## Extract Embedded GDI Bitmaps

If the `.spl` file starts with `PK` or contains paths such as `Documents/1/Pages/1.fpage`, it is an XPS package. Run:

```powershell
python tools\xps_spool_extract.py captured.spl --out xps_extract
```

The tool writes:

- extracted image resources
- FixedPage XML files
- print-ticket XML files
- `manifest.json` with image dimensions, sizes, and ICC-profile presence

If the `.spl` file is EMF-backed instead, run:

```powershell
python tools\emf_stretchdibits_extract.py captured.spl --out spool_extract
```

The tool writes:

- extracted PNG files for supported `EMR_STRETCHDIBITS` records
- `manifest.json` with record offsets, source rectangles, destination rectangles, bit depth, compression, and raster operation

Use the same process for PrintCatalog and Mr. Photo, then compare the extracted PNGs and `manifest.json` files.

## Important Limits

- If the spool file is RAW printer language, XPS, or PDF instead of EMF, this extractor will not find GDI bitmap records.
- This cannot reproduce physical printer darkness caused after the spool stage by the real printer driver, ICC/profile conversion, paper type, ink limits, or hardware calibration.
- A full custom virtual printer/port monitor is possible, but it is much heavier: it requires Windows print-driver or print-processor work, installation as an administrator, and often driver signing.
