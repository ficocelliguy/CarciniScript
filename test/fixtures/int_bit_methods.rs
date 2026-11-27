// Integer bit twiddling and overflow-reporting methods: `rotate_*`, `swap_bytes` / `to_be`,
// `reverse_bits`, `*_shl` / `*_shr`, `overflowing_*`, `checked_*_euclid`, `div_ceil`,
// `next_multiple_of`, `cast_signed` / `cast_unsigned`, `u8::eq_ignore_ascii_case`, float byte
// conversions, and the same named by path (`u16::to_be_bytes(x)`, `u32::max_value()`).
fn main() {
    let a: u32 = 0x1234_5678;
    println!("{:#x} {:#x} {:#x} {:#x}", a.to_le(), a.to_be(), a.swap_bytes(), u32::from_be(a));
    println!("{:#x} {:#x}", 0x1234u16.to_be(), (-2i32).swap_bytes());
    println!("{} {} {}", 7usize.div_ceil(2), 8usize.div_ceil(4), 0u8.div_ceil(3));
    println!("{} {} {}", 13usize.next_multiple_of(5), 15u32.next_multiple_of(5), 7u8.checked_next_multiple_of(0).is_none());
    println!("{:#x} {:#x} {:#x}", 0x8000_0001u32.rotate_left(1), 0x8000_0001u32.rotate_right(1), 0b1000_0001u8.rotate_left(3));
    println!("{} {} {}", (1u64 << 40).rotate_right(8), 1u128.rotate_right(1) == 1u128 << 127, (-1i32).rotate_left(5));
    println!("{} {} {}", 1u32.wrapping_shl(33), 0x80u8.wrapping_shr(9), (-8i32).wrapping_shr(1));
    println!("{:?} {:?} {:?}", 1u32.checked_shl(31), 1u32.checked_shl(32), (-128i8).checked_shr(7));
    println!("{:?} {:?} {:?}", 250u8.overflowing_add(10), 250u8.overflowing_add(5), 5u8.overflowing_sub(6));
    println!("{:?} {:?} {:?}", i32::MAX.overflowing_add(1), 100_000i32.overflowing_mul(100_000), (-7i32).overflowing_neg());
    println!("{:?} {:?} {:?}", i8::MIN.overflowing_neg(), 3u8.overflowing_pow(6), 1u16.overflowing_shl(17));
    println!("{:?} {:?}", (1u64 << 50).overflowing_add(1 << 50), (-5i64).overflowing_sub(3));
    println!("{:?} {:?}", u128::MAX.overflowing_add(1), (1u128 << 100).overflowing_mul(2));
    println!("{:?} {:?} {:?}", (-7i32).checked_div_euclid(2), (-7i32).checked_rem_euclid(2), 7i32.checked_div_euclid(0));
    println!("{:?} {:?}", (-7i128).checked_rem_euclid(2), 9u128.checked_div_euclid(0));
    println!("{} {} {}", 200u8.cast_signed(), (-1i32).cast_unsigned(), 3u64.cast_signed());
    println!("{} {}", (-1i128).cast_unsigned() == u128::MAX, u8::MAX.cast_signed());
    println!("{:#x} {:#x} {:#x}", 0b0000_0001u8.reverse_bits(), 0x0001u16.reverse_bits(), 1u32.reverse_bits());
    println!("{} {}", 1u128.reverse_bits() == 1u128 << 127, (-2i8).reverse_bits());
    println!("{} {} {}", b'a'.eq_ignore_ascii_case(&b'A'), b'z'.eq_ignore_ascii_case(&b'Y'), b'1'.eq_ignore_ascii_case(&b'1'));
    println!("{:?} {:?}", 1.5f32.to_le_bytes(), 1.5f64.to_be_bytes());
    println!("{} {}", f32::from_le_bytes([0, 0, 192, 63]), f64::from_be_bytes(1.5f64.to_be_bytes()));
    println!("{:?} {:?}", u16::to_be_bytes(0x1234), u64::to_le_bytes(258));
    println!("{} {} {}", u32::max_value(), i128::max_value(), i16::min_value());
    println!("{} {} {}", usize::rotate_left(1, 3), u32::wrapping_add(u32::MAX, 3), u32::count_ones(7));
    let c = unsafe { char::from_u32_unchecked(0x41) };
    println!("{c} {:#x} {:#x}", 0x1234u128.swap_bytes() >> 112, 0x1234u128.to_be() >> 112);
    println!("{} {}", 10u128.div_ceil(3), 7u128.next_multiple_of(4));
}
