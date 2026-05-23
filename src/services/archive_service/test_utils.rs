pub(crate) fn create_single_file_zip_with_raw_name(raw_name: &[u8], content: &[u8]) -> Vec<u8> {
    let content_crc = crc32(content);
    let compressed_size: u32 = content.len().try_into().expect("test content fits u32");
    let uncompressed_size = compressed_size;
    let name_len: u16 = raw_name.len().try_into().expect("test filename fits u16");

    let mut bytes = Vec::new();
    push_u32(&mut bytes, 0x0403_4b50);
    push_u16(&mut bytes, 10);
    push_u16(&mut bytes, 0);
    push_u16(&mut bytes, 0);
    push_u16(&mut bytes, 0);
    push_u16(&mut bytes, 0);
    push_u32(&mut bytes, content_crc);
    push_u32(&mut bytes, compressed_size);
    push_u32(&mut bytes, uncompressed_size);
    push_u16(&mut bytes, name_len);
    push_u16(&mut bytes, 0);
    bytes.extend_from_slice(raw_name);
    bytes.extend_from_slice(content);

    let central_directory_offset: u32 = bytes
        .len()
        .try_into()
        .expect("test central directory offset fits u32");
    push_u32(&mut bytes, 0x0201_4b50);
    push_u16(&mut bytes, 20);
    push_u16(&mut bytes, 10);
    push_u16(&mut bytes, 0);
    push_u16(&mut bytes, 0);
    push_u16(&mut bytes, 0);
    push_u16(&mut bytes, 0);
    push_u32(&mut bytes, content_crc);
    push_u32(&mut bytes, compressed_size);
    push_u32(&mut bytes, uncompressed_size);
    push_u16(&mut bytes, name_len);
    push_u16(&mut bytes, 0);
    push_u16(&mut bytes, 0);
    push_u16(&mut bytes, 0);
    push_u16(&mut bytes, 0);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 0);
    bytes.extend_from_slice(raw_name);

    let central_directory_size: u32 = (bytes.len()
        - usize::try_from(central_directory_offset)
            .expect("test central directory offset fits usize"))
    .try_into()
    .expect("test central directory size fits u32");
    push_u32(&mut bytes, 0x0605_4b50);
    push_u16(&mut bytes, 0);
    push_u16(&mut bytes, 0);
    push_u16(&mut bytes, 1);
    push_u16(&mut bytes, 1);
    push_u32(&mut bytes, central_directory_size);
    push_u32(&mut bytes, central_directory_offset);
    push_u16(&mut bytes, 0);

    bytes
}

pub(crate) fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}
