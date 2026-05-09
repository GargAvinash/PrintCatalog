#!/usr/bin/env python
"""
Extract image resources and page XML from an XPS-backed .spl file.

Usage:
  python tools/xps_spool_extract.py 00004.SPL --out pc_xps
  python tools/xps_spool_extract.py 00005.SPL --out mr_xps

Many modern Windows queues spool as XPS packages. These files start with the
ZIP signature "PK" and contain FixedPage XML plus image resources.
"""

from __future__ import annotations

import argparse
import json
import zipfile
from dataclasses import asdict, dataclass
from pathlib import Path

from PIL import Image


IMAGE_EXTENSIONS = {".jpg", ".jpeg", ".png", ".bmp", ".tif", ".tiff", ".wdp", ".jxr"}
PAGE_EXTENSIONS = {".fpage"}
TICKET_SUFFIX = "_PT.xml"


@dataclass
class PackageEntry:
    package_path: str
    output: str
    size: int
    compressed_size: int


@dataclass
class ImageEntry(PackageEntry):
    width: int
    height: int
    format: str | None
    mode: str
    icc_profile_bytes: int


def safe_name(package_path: str) -> str:
    return package_path.replace("/", "__").replace("\\", "__")


def extract_xps_spool(input_path: Path, out_dir: Path) -> dict[str, object]:
    out_dir.mkdir(parents=True, exist_ok=True)
    images_dir = out_dir / "images"
    pages_dir = out_dir / "pages"
    tickets_dir = out_dir / "tickets"
    images_dir.mkdir(exist_ok=True)
    pages_dir.mkdir(exist_ok=True)
    tickets_dir.mkdir(exist_ok=True)

    image_entries: list[ImageEntry] = []
    page_entries: list[PackageEntry] = []
    ticket_entries: list[PackageEntry] = []

    with zipfile.ZipFile(input_path) as package:
        for info in package.infolist():
            package_path = info.filename
            suffix = Path(package_path).suffix.lower()

            if suffix in IMAGE_EXTENSIONS:
                output_path = images_dir / safe_name(package_path)
                output_path.write_bytes(package.read(package_path))
                with Image.open(output_path) as image:
                    image_entries.append(
                        ImageEntry(
                            package_path=package_path,
                            output=str(output_path),
                            size=info.file_size,
                            compressed_size=info.compress_size,
                            width=image.width,
                            height=image.height,
                            format=image.format,
                            mode=image.mode,
                            icc_profile_bytes=len(image.info.get("icc_profile", b"")),
                        )
                    )
            elif suffix in PAGE_EXTENSIONS:
                output_path = pages_dir / safe_name(package_path)
                output_path.write_bytes(package.read(package_path))
                page_entries.append(
                    PackageEntry(package_path, str(output_path), info.file_size, info.compress_size)
                )
            elif package_path.endswith(TICKET_SUFFIX):
                output_path = tickets_dir / safe_name(package_path)
                output_path.write_bytes(package.read(package_path))
                ticket_entries.append(
                    PackageEntry(package_path, str(output_path), info.file_size, info.compress_size)
                )

    return {
        "input": str(input_path),
        "images": [asdict(entry) for entry in image_entries],
        "pages": [asdict(entry) for entry in page_entries],
        "print_tickets": [asdict(entry) for entry in ticket_entries],
    }


def main() -> None:
    parser = argparse.ArgumentParser(description="Extract images from an XPS-backed .spl file.")
    parser.add_argument("input", type=Path)
    parser.add_argument("--out", type=Path, default=Path("xps_spool_extract"))
    args = parser.parse_args()

    manifest = extract_xps_spool(args.input, args.out)
    manifest_path = args.out / "manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")

    print(f"Extracted {len(manifest['images'])} image(s)")
    print(f"Extracted {len(manifest['pages'])} page XML file(s)")
    print(f"Extracted {len(manifest['print_tickets'])} print ticket(s)")
    print(f"Wrote {manifest_path}")


if __name__ == "__main__":
    main()
