//! A desktop's picture as it is on the machine: every rectangle the machine draws is put
//! where it belongs.

use crate::update::Rectangle;
use crate::{interleaved, planar, Error};

/// A picture: rows from the top down, four bytes a pixel: blue, green, red, and a fourth
/// that is always full.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    /// How wide it is, in pixels.
    pub width: usize,
    /// How high.
    pub height: usize,
    /// Its pixels.
    pub pixels: Vec<u8>,
}

impl Picture {
    /// A black picture of this size.
    pub fn new(width: usize, height: usize) -> Self {
        let mut pixels = vec![0; width * height * 4];
        pixels.as_chunks_mut::<4>().0.iter_mut().for_each(|pixel| pixel[3] = 0xff);
        Picture { width, height, pixels }
    }

    /// Puts a rectangle where it belongs. What of it lies outside the picture is left out.
    pub fn draw(&mut self, rectangle: &Rectangle) -> Result<(), Error> {
        let (carried_width, carried_height) = (usize::from(rectangle.width), usize::from(rectangle.height));
        // a machine fills its rows up to whole words, and no further: whatever says it
        // carries more than the desktop holds is not asked for the memory it names
        if carried_width > self.width + 64 || carried_height > self.height + 64 {
            return Err(Error::NotRdp("a rectangle carries more than the desktop holds"));
        }
        let drawn = match (rectangle.bits, rectangle.compressed) {
            (32, true) => planar::unpack(&rectangle.carries, carried_width, carried_height)?,
            (32, false) => {
                // as it is: rows from the bottom up
                let row = carried_width * 4;
                if row == 0 || rectangle.carries.len() < row * carried_height {
                    return Err(Error::NotRdp("a rectangle carries less than its size"));
                }
                let mut rows: Vec<u8> = rectangle.carries[..row * carried_height].rchunks_exact(row).flatten().copied().collect();
                rows.as_chunks_mut::<4>().0.iter_mut().for_each(|pixel| pixel[3] = 0xff);
                rows
            }
            (16, true) => interleaved::unpack(&rectangle.carries, carried_width, carried_height)?,
            (16, false) => {
                // as it is: rows from the bottom up, each filled up to whole words
                let row = (carried_width * 2).next_multiple_of(4);
                if row == 0 || rectangle.carries.len() < row * carried_height {
                    return Err(Error::NotRdp("a rectangle carries less than its size"));
                }
                rectangle.carries[..row * carried_height]
                    .rchunks_exact(row)
                    .flat_map(|row| row[..carried_width * 2].as_chunks::<2>().0)
                    .flat_map(|pixel| interleaved::from_16_bits(u16::from_le_bytes(*pixel)))
                    .collect()
            }
            _ => return Err(Error::NotYet("draws with 8, 15 or 24 bits a pixel")),
        };
        let (left, top) = (usize::from(rectangle.left), usize::from(rectangle.top));
        if rectangle.right < rectangle.left || rectangle.bottom < rectangle.top {
            return Err(Error::NotRdp("a rectangle ends before it begins"));
        }
        let width = (usize::from(rectangle.right) - left + 1).min(carried_width).min(self.width.saturating_sub(left));
        let height = (usize::from(rectangle.bottom) - top + 1).min(carried_height).min(self.height.saturating_sub(top));
        if width == 0 {
            return Ok(());
        }
        for row in 0..height {
            let from = row * carried_width * 4;
            let to = ((top + row) * self.width + left) * 4;
            self.pixels[to..to + width * 4].copy_from_slice(&drawn[from..from + width * 4]);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rectangle(left: u16, top: u16, wide: u16, high: u16, compressed: bool, carries: &[u8]) -> Rectangle {
        Rectangle {
            left,
            top,
            right: left + wide - 1,
            bottom: top + high - 1,
            width: wide,
            height: high,
            bits: 32,
            compressed,
            carries: carries.to_vec(),
        }
    }

    fn pixel(picture: &Picture, column: usize, row: usize) -> [u8; 4] {
        picture.pixels[(row * picture.width + column) * 4..][..4].try_into().unwrap()
    }

    #[test]
    fn a_rectangle_goes_where_it_belongs_the_right_way_up() {
        let mut picture = Picture::new(4, 3);
        // two by two as it is, from the bottom row up: the bottom row first
        let carries = [1, 1, 1, 0, 2, 2, 2, 0, /* the top row */ 3, 3, 3, 0, 4, 4, 4, 0];
        picture.draw(&rectangle(1, 1, 2, 2, false, &carries)).unwrap();
        assert_eq!(pixel(&picture, 1, 1), [3, 3, 3, 255]);
        assert_eq!(pixel(&picture, 2, 1), [4, 4, 4, 255]);
        assert_eq!(pixel(&picture, 1, 2), [1, 1, 1, 255]);
        assert_eq!(pixel(&picture, 0, 0), [0, 0, 0, 255], "the rest is as it was");
        // the same packed in planes
        let packed = [0x20, 10, 20, 30, 40, 11, 21, 31, 41, 12, 22, 32, 42, 0];
        picture.draw(&rectangle(0, 0, 2, 2, true, &packed)).unwrap();
        assert_eq!(pixel(&picture, 0, 0), [32, 31, 30, 255]);
        assert_eq!(pixel(&picture, 1, 1), [22, 21, 20, 255]);
    }

    #[test]
    fn a_rectangle_of_16_bits_a_pixel_is_drawn_in_full_colours() {
        let mut picture = Picture::new(4, 3);
        // three by two as it is: the bottom row first, each row filled up to whole words
        let (red, green, blue, white) = ([0x00, 0xf8], [0xe0, 0x07], [0x1f, 0x00], [0xff, 0xff]);
        let carries = [&blue[..], &blue, &white, &[0xaa, 0xaa], /* the top row */ &red, &green, &blue, &[0xaa, 0xaa]].concat();
        let mut as_it_is = rectangle(1, 0, 3, 2, false, &carries);
        as_it_is.bits = 16;
        picture.draw(&as_it_is).unwrap();
        assert_eq!(pixel(&picture, 1, 0), [0, 0, 255, 255]);
        assert_eq!(pixel(&picture, 2, 0), [0, 255, 0, 255]);
        assert_eq!(pixel(&picture, 3, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(&picture, 3, 1), [255, 255, 255, 255]);
        assert_eq!(pixel(&picture, 0, 0), [0, 0, 0, 255], "the rest is as it was");
        // the same in runs: the bottom row written out, the top row one colour
        let packed = [0x83, 0x1f, 0x00, 0x1f, 0x00, 0xff, 0xff, /* three times red */ 0x63, 0x00, 0xf8];
        let mut in_runs = rectangle(0, 1, 3, 2, true, &packed);
        in_runs.bits = 16;
        picture.draw(&in_runs).unwrap();
        assert_eq!(pixel(&picture, 0, 1), [0, 0, 255, 255]);
        assert_eq!(pixel(&picture, 2, 2), [255, 255, 255, 255]);
        // one row short
        as_it_is.carries.truncate(15);
        assert!(picture.draw(&as_it_is).is_err());
    }

    #[test]
    fn what_lies_outside_the_picture_is_left_out() {
        let mut picture = Picture::new(3, 2);
        picture.draw(&rectangle(2, 1, 2, 2, false, &[9; 16])).unwrap();
        assert_eq!(pixel(&picture, 2, 1), [9, 9, 9, 255]);
        picture.draw(&rectangle(7, 7, 2, 2, false, &[9; 16])).unwrap();
        // beside the picture, in its last row
        let before = picture.clone();
        picture.draw(&rectangle(9, 1, 2, 1, false, &[9; 8])).unwrap();
        assert_eq!(picture, before);
        // what carries more than where it goes: only where it goes is drawn
        let mut wider = rectangle(0, 0, 4, 1, false, &[5; 16]);
        wider.right = 1;
        picture.draw(&wider).unwrap();
        assert_eq!(pixel(&picture, 1, 0), [5, 5, 5, 255]);
        assert_eq!(pixel(&picture, 2, 0), [0, 0, 0, 255]);
    }

    #[test]
    fn what_is_too_short_or_of_another_depth_is_refused() {
        let mut picture = Picture::new(4, 4);
        assert!(picture.draw(&rectangle(0, 0, 2, 2, false, &[1; 15])).is_err());
        let mut deep = rectangle(0, 0, 2, 2, false, &[1; 16]);
        deep.bits = 24;
        assert_eq!(picture.draw(&deep), Err(Error::NotYet("draws with 8, 15 or 24 bits a pixel")));
        let mut backwards = rectangle(2, 2, 1, 1, false, &[1; 4]);
        backwards.right = 1;
        assert!(picture.draw(&backwards).is_err());
        // a rectangle that says it is as large as a rectangle can be, and carries next to nothing
        let mut vast = rectangle(0, 0, 1, 1, true, &[0x30, 0x06]);
        (vast.width, vast.height) = (u16::MAX, u16::MAX);
        assert_eq!(picture.draw(&vast), Err(Error::NotRdp("a rectangle carries more than the desktop holds")));
    }
}
