import fitz # PyMuPDF
from PIL import Image
import io
import os

def analyze_pdf(pdf_path, name):
    print(f"--- Analyzing {name} ({pdf_path}) ---")
    if not os.path.exists(pdf_path):
        print(f"File not found: {pdf_path}")
        return
    
    doc = fitz.open(pdf_path)
    print(f"Number of pages: {len(doc)}")
    
    for i in range(len(doc)):
        page = doc[i]
        image_list = page.get_images(full=True)
        print(f"Page {i+1} has {len(image_list)} images.")
        
        for img_index, img in enumerate(image_list):
            xref = img[0]
            base_image = doc.extract_image(xref)
            image_bytes = base_image["image"]
            image_ext = base_image["ext"]
            colorspace = base_image["colorspace"]
            width = base_image["width"]
            height = base_image["height"]
            bpc = base_image["bpc"]
            
            print(f"  Image {img_index+1} (xref {xref}):")
            print(f"    Format: {image_ext}")
            print(f"    Dimensions: {width}x{height}")
            print(f"    Colorspace: {colorspace} (bpc: {bpc})")
            print(f"    Size: {len(image_bytes)} bytes")
            
            # Load into Pillow to check ICC profile
            try:
                pil_img = Image.open(io.BytesIO(image_bytes))
                icc = pil_img.info.get("icc_profile")
                print(f"    Has ICC profile: {'Yes' if icc else 'No'}")
                if icc:
                    print(f"    ICC profile size: {len(icc)} bytes")
                
                # We can also check mode
                print(f"    Pillow mode: {pil_img.mode}")
            except Exception as e:
                print(f"    Error analyzing with Pillow: {e}")

def analyze_image(img_path):
    print(f"--- Analyzing Original Image ({img_path}) ---")
    if not os.path.exists(img_path):
        print(f"File not found: {img_path}")
        return
    
    try:
        pil_img = Image.open(img_path)
        icc = pil_img.info.get("icc_profile")
        print(f"Dimensions: {pil_img.width}x{pil_img.height}")
        print(f"Format: {pil_img.format}")
        print(f"Mode: {pil_img.mode}")
        print(f"Has ICC profile: {'Yes' if icc else 'No'}")
        if icc:
            print(f"ICC profile size: {len(icc)} bytes")
    except Exception as e:
        print(f"Error analyzing image: {e}")

if __name__ == "__main__":
    analyze_image("D:\\PrintCatalog\\statue_of_liberty.JPG")
    analyze_pdf("D:\\PrintCatalog\\mr_photo.pdf", "Mr Photo PDF")
    analyze_pdf("D:\\PrintCatalog\\PrintCatalog13.pdf", "Our App PDF")
