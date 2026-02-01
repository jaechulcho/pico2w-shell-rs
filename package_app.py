import sys
import zlib
import struct

def package(input_file, output_file):
    # 1. Read the raw binary
    with open(input_file, 'rb') as f:
        data = f.read()

    length = len(data)
    
    # 2. Calculate CRC32 (ISO-HDLC / zlib compatible)
    # zlib.crc32 returns the unsigned 32-bit CRC32
    crc_val = zlib.crc32(data) & 0xFFFFFFFF
    
    print(f"Packaging {input_file}...")
    print(f"Length: {length} bytes")
    print(f"CRC32:  0x{crc_val:08x}")

    # 3. Create Metadata Header (256 bytes)
    # [Magic "APPS" (4) | Length (4, LE) | CRC32 (4, LE) | Padding (244)]
    magic = b"APPS"
    header = magic + struct.pack("<II", length, crc_val)
    header = header.ljust(256, b'\xFF')

    # 4. Write final binary
    with open(output_file, 'wb') as f:
        f.write(header)
        f.write(data)

    print(f"Final binary created: {output_file}")

if __name__ == "__main__":
    if len(sys.argv) < 3:
        print("Usage: python package_app.py <input.bin> <output.bin>")
        sys.exit(1)
    
    package(sys.argv[1], sys.argv[2])
