// `size_of_val` / `size_of` / `align_of` of arrays, slices, boxes, `Vec`s, tuples and structs:
// the layout rustc gives them (Rust by Example `primitives/array`, `std/box`).
use std::mem;

#[derive(Debug, Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
}

struct Rectangle {
    top_left: Point,
    bottom_right: Point,
}

struct Mixed(u8, u32, u16);
struct Unit;

fn slice_bytes(s: &[i32]) -> usize {
    mem::size_of_val(s)
}

fn main() {
    let xs: [i32; 5] = [1, 2, 3, 4, 5];
    println!("{}", mem::size_of_val(&xs));
    let ys = [0u8; 500];
    println!("{}", mem::size_of_val(&ys));
    let grid = [[0u16; 3]; 4];
    println!("{}", mem::size_of_val(&grid));
    println!("{}", mem::size_of_val(&xs[1..4]));
    println!("{}", slice_bytes(&xs[2..]));
    let s: &[i32] = &xs[1..3];
    println!("{}", mem::size_of_val(s));
    let v = vec![1, 2, 3];
    println!("{}", mem::size_of_val(&v));

    let point = Point { x: 0.0, y: 0.0 };
    let rectangle = Rectangle { top_left: point, bottom_right: Point { x: 3.0, y: -4.0 } };
    let boxed_point = Box::new(point);
    let boxed_rectangle: Box<Rectangle> = Box::new(rectangle);
    let box_in_a_box = Box::new(Box::new(point));
    let unboxed_point: Point = *boxed_point;
    println!("Point occupies {} bytes on the stack", mem::size_of_val(&point));
    println!("Rectangle occupies {} bytes on the stack", mem::size_of_val(&boxed_rectangle.top_left) * 2);
    println!("Boxed point occupies {} bytes on the stack", mem::size_of_val(&boxed_point));
    println!("Boxed rectangle occupies {} bytes on the stack", mem::size_of_val(&boxed_rectangle));
    println!("Boxed box occupies {} bytes on the stack", mem::size_of_val(&box_in_a_box));
    println!("Unboxed point occupies {} bytes on the stack", mem::size_of_val(&unboxed_point));

    println!("{} {}", mem::size_of::<Mixed>(), mem::align_of::<Mixed>());
    println!("{} {}", mem::size_of::<Unit>(), mem::align_of::<()>());
    println!("{} {}", mem::size_of::<(u8, f64)>(), mem::size_of::<[u32; 3]>());
    println!("{} {}", mem::size_of::<&str>(), mem::size_of::<Box<[u8]>>());
    println!("{}", mem::align_of_val(&xs[1..4]));
    println!("{}", std::mem::size_of_val(&7u16));
    let t = (1u8, 2u64);
    println!("{}", mem::size_of_val(&t));
}
