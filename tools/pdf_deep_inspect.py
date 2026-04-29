import fitz
import hashlib

def get_hash(bytes_data):
    return hashlib.md5(bytes_data).hexdigest()

def analyze_deep(pdf_path, name):
    print(f"\n--- Deep Analysis of {name} ({pdf_path}) ---")
    doc = fitz.open(pdf_path)
    page = doc[0]
    
    # 1. Print page contents
    contents = page.read_contents()
    print("Page Contents Stream:")
    # print up to 500 chars to avoid massive output
    try:
        print(contents.decode('latin1')[:500])
    except:
        print(contents[:500])
        
    # 2. Get image hash and details
    image_list = page.get_images(full=True)
    xref = image_list[0][0]
    base_image = doc.extract_image(xref)
    
    print(f"Image Hash: {get_hash(base_image['image'])}")
    
    # 3. Check XObject properties
    img_obj = doc.xref_object(xref)
    print(f"Image XObject Metadata:\n{img_obj}")

orig_data = open('D:\\PrintCatalog\\statue_of_liberty.JPG', 'rb').read()
print(f"\nOriginal Image Hash: {get_hash(orig_data)}")
analyze_deep("D:\\PrintCatalog\\mr_photo.pdf", "Mr Photo PDF")
analyze_deep("D:\\PrintCatalog\\PrintCatalog13.pdf", "Our App PDF")
