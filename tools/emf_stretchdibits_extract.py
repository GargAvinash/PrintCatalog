#!/usr/bin/env python
"""
Extract EMR_STRETCHDIBITS bitmap payloads from EMF or EMF-backed .spl files.

Usage:
  python tools/emf_stretchdibits_extract.py capture.spl --out emf_extract
  python tools/emf_stretchdibits_extract.py page.emf --out emf_extract

This is useful for comparing what GDI applications handed to the spooler.
It is not a physical printer simulator: it sees EMF spool data before the
printer driver converts that data into device-specific output.
"""

from __future__ import annotations

import argparse
import json
import struct
from dataclasses import asdict, dataclass
from pathlib import Path

from PIL import Image


EMR_HEADER = 1
EMR_EOF = 14
EMR_STRETCHDIBITS = 81
EMF_SIGNATURE = 0x464D4520  # " EMF"
BI_RGB = 0


@dataclass
class ExtractedBitmap:
    page_index: int
    record_index: int
    record_offset: int
    output: str | None
    width: int
    height: int
    bit_count: int
    compression: int
    source_rect: tuple[int, int, int, int]
    dest_rect: tuple[int, int, int, int]
    rop: int
    skipped_reason: str | None = None


def u32(data: bytes, offset: int) -> int:
    return struct.unpack_from("<I", data, offset)[0]


def i32(data: bytes, offset: int) -> int:
    return struct.unpack_from("<i", data, offset)[0]


def is_emf_header(data: bytes, offset: int) -> bool:
    if offset + 108 > len(data):
        return False
    record_type = u32(data, offset)
    record_size = u32(data, offset + 4)
    signature = u32(data, offset + 40)
    return (
        record_type == EMR_HEADER
        and signature == EMF_SIGNATURE
        and 88 <= record_size <= len(data) - offset
    )


def find_emf_streams(data: bytes) -> list[int]:
    offsets: list[int] = []
    pos = 0
    while True:
        pos = data.find(b"\x01\x00\x00\x00", pos)
        if pos < 0:
            break
        if is_emf_header(data, pos):
            offsets.append(pos)
        pos += 4
    return offsets


def dib_to_image(bmi: bytes, bits: bytes) -> tuple[Image.Image | None, dict[str, int | str | None]]:
    if len(bmi) < 40:
        return None, {"skipped_reason": "BITMAPINFOHEADER is shorter than 40 bytes"}

    header_size = u32(bmi, 0)
    width = i32(bmi, 4)
    signed_height = i32(bmi, 8)
    planes = struct.unpack_from("<H", bmi, 12)[0]
    bit_count = struct.unpack_from("<H", bmi, 14)[0]
    compression = u32(bmi, 16)
    height = abs(signed_height)

    info: dict[str, int | str | None] = {
        "width": width,
        "height": height,
        "bit_count": bit_count,
        "compression": compression,
        "skipped_reason": None,
    }

    if header_size < 40 or width <= 0 or height <= 0 or planes != 1:
        info["skipped_reason"] = "Unsupported BITMAPINFOHEADER geometry"
        return None, info

    if compression != BI_RGB:
        info["skipped_reason"] = f"Unsupported compressed DIB format {compression}"
        return None, info

    top_down = signed_height < 0

    if bit_count == 24:
        stride = ((width * 3 + 3) // 4) * 4
        required = stride * height
        if len(bits) < required:
            info["skipped_reason"] = "DIB bits are truncated"
            return None, info
        image = Image.frombuffer(
            "RGB",
            (width, height),
            bits[:required],
            "raw",
            "BGR",
            stride,
            1 if top_down else -1,
        ).copy()
        return image, info

    if bit_count == 32:
        stride = width * 4
        required = stride * height
        if len(bits) < required:
            info["skipped_reason"] = "DIB bits are truncated"
            return None, info
        image = Image.frombuffer(
            "RGB",
            (width, height),
            bits[:required],
            "raw",
            "BGRX",
            stride,
            1 if top_down else -1,
        ).copy()
        return image, info

    info["skipped_reason"] = f"Unsupported bit depth {bit_count}"
    return None, info


def extract_from_stream(
    data: bytes,
    stream_offset: int,
    page_index: int,
    out_dir: Path,
) -> list[ExtractedBitmap]:
    results: list[ExtractedBitmap] = []
    record_offset = stream_offset
    record_index = 0

    while record_offset + 8 <= len(data):
        record_type = u32(data, record_offset)
        record_size = u32(data, record_offset + 4)
        if record_size < 8 or record_offset + record_size > len(data):
            break

        if record_type == EMR_STRETCHDIBITS and record_size >= 80:
            base = record_offset
            x_dest = i32(data, base + 24)
            y_dest = i32(data, base + 28)
            x_src = i32(data, base + 32)
            y_src = i32(data, base + 36)
            cx_src = i32(data, base + 40)
            cy_src = i32(data, base + 44)
            off_bmi = u32(data, base + 48)
            cb_bmi = u32(data, base + 52)
            off_bits = u32(data, base + 56)
            cb_bits = u32(data, base + 60)
            rop = u32(data, base + 68)
            cx_dest = i32(data, base + 72)
            cy_dest = i32(data, base + 76)

            bmi_start = base + off_bmi
            bits_start = base + off_bits
            bmi = data[bmi_start : bmi_start + cb_bmi]
            bits = data[bits_start : bits_start + cb_bits]
            image, info = dib_to_image(bmi, bits)

            output: str | None = None
            if image is not None:
                output_path = out_dir / f"page{page_index:02d}_record{record_index:04d}.png"
                image.save(output_path)
                output = str(output_path)

            results.append(
                ExtractedBitmap(
                    page_index=page_index,
                    record_index=record_index,
                    record_offset=record_offset,
                    output=output,
                    width=int(info["width"]),
                    height=int(info["height"]),
                    bit_count=int(info["bit_count"]),
                    compression=int(info["compression"]),
                    source_rect=(x_src, y_src, cx_src, cy_src),
                    dest_rect=(x_dest, y_dest, cx_dest, cy_dest),
                    rop=rop,
                    skipped_reason=info["skipped_reason"],
                )
            )

        record_index += 1
        record_offset += record_size
        if record_type == EMR_EOF:
            break

    return results


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Extract EMR_STRETCHDIBITS bitmap payloads from EMF/.spl files."
    )
    parser.add_argument("input", type=Path, help="EMF or EMF-backed spool file")
    parser.add_argument("--out", type=Path, default=Path("emf_stretchdibits_extract"))
    args = parser.parse_args()

    data = args.input.read_bytes()
    offsets = find_emf_streams(data)
    args.out.mkdir(parents=True, exist_ok=True)

    all_results: list[ExtractedBitmap] = []
    for page_index, offset in enumerate(offsets, 1):
        all_results.extend(extract_from_stream(data, offset, page_index, args.out))

    manifest = {
        "input": str(args.input),
        "emf_stream_offsets": offsets,
        "stretchdibits_records": [asdict(result) for result in all_results],
    }
    manifest_path = args.out / "manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")

    saved = sum(1 for result in all_results if result.output)
    skipped = len(all_results) - saved
    print(f"Found {len(offsets)} EMF stream(s)")
    print(f"Found {len(all_results)} EMR_STRETCHDIBITS record(s)")
    print(f"Saved {saved} bitmap(s), skipped {skipped}")
    print(f"Wrote {manifest_path}")


if __name__ == "__main__":
    main()
