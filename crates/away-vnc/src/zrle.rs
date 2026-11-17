//! The rectangles a machine packs most densely (RFC 6143, 7.7.6): cut into tiles of 64
//! by 64 pixels, each written the shortest of five ways, and all of it packed once more
//! as one stream for the whole connection. This module reads the tiles; the stream is
//! unpacked by whoever holds the connection.
//!
//! A pixel is written as three bytes, blue, green and red, which is what the protocol
//! makes of the pixels Away asks for.

use crate::picture::{Area, Picture};
use crate::Error;

const WRONG: Error = Error::NotVnc("a packed rectangle is not built as one");

/// How wide and how high a tile is, at most.
pub const TILE: usize = 64;

struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    fn bytes(&mut self, count: usize) -> Result<&'a [u8], Error> {
        let (bytes, rest) = self.rest.split_at_checked(count).ok_or(WRONG)?;
        self.rest = rest;
        Ok(bytes)
    }

    fn byte(&mut self) -> Result<u8, Error> {
        Ok(self.bytes(1)?[0])
    }

    fn pixel(&mut self) -> Result<[u8; 3], Error> {
        let pixel = self.bytes(3)?;
        Ok([pixel[0], pixel[1], pixel[2]])
    }

    fn pixels(&mut self, count: usize) -> Result<Vec<[u8; 3]>, Error> {
        (0..count).map(|_| self.pixel()).collect()
    }

    /// How long a run is: one more than the sum of its bytes, of which every one but
    /// the last is full.
    fn run(&mut self) -> Result<usize, Error> {
        let mut length = 1;
        loop {
            let byte = self.byte()?;
            length += usize::from(byte);
            if byte != 255 {
                return Ok(length);
            }
            if length > TILE * TILE {
                return Err(WRONG);
            }
        }
    }
}

/// Puts an unpacked rectangle into the picture, tile by tile. The area lies inside the
/// picture.
pub fn draw(unpacked: &[u8], area: Area, picture: &mut Picture) -> Result<(), Error> {
    let mut reader = Reader { rest: unpacked };
    for top in (area.top..area.top + area.height).step_by(TILE) {
        let high = TILE.min(area.top + area.height - top);
        for left in (area.left..area.left + area.width).step_by(TILE) {
            let wide = TILE.min(area.left + area.width - left);
            let count = wide * high;
            // a run of one colour, from one pixel of the tile on, row by row
            let run = |picture: &mut Picture, from: usize, length: usize, pixel: [u8; 3]| {
                if from + length > count {
                    return Err(WRONG);
                }
                (from..from + length).for_each(|nth| picture.set(left + nth % wide, top + nth / wide, pixel));
                Ok(from + length)
            };
            match reader.byte()? {
                // every pixel as it is
                0 => {
                    for nth in 0..count {
                        picture.set(left + nth % wide, top + nth / wide, reader.pixel()?);
                    }
                }
                // one colour
                1 => picture.fill(Area { left, top, width: wide, height: high }, reader.pixel()?),
                // a few colours, and every pixel as the number of its colour, in as few bits as hold it
                size @ 2..=16 => {
                    let colours = reader.pixels(usize::from(size))?;
                    let bits = match size {
                        2 => 1,
                        3..=4 => 2,
                        _ => 4,
                    };
                    for row in 0..high {
                        // each row begins on a byte of its own
                        let packed = reader.bytes((wide * bits).div_ceil(8))?;
                        for column in 0..wide {
                            let at = column * bits;
                            let number = (packed[at / 8] >> (8 - bits - at % 8)) & ((1 << bits) - 1);
                            picture.set(left + column, top + row, *colours.get(usize::from(number)).ok_or(WRONG)?);
                        }
                    }
                }
                // runs, each with its colour
                128 => {
                    let mut at = 0;
                    while at < count {
                        let pixel = reader.pixel()?;
                        let length = reader.run()?;
                        at = run(picture, at, length, pixel)?;
                    }
                }
                // some colours, and runs, each with the number of its colour
                size @ 130..=255 => {
                    let colours = reader.pixels(usize::from(size - 128))?;
                    let mut at = 0;
                    while at < count {
                        let number = reader.byte()?;
                        // a number with its highest bit set is followed by how long its run is
                        let length = if number & 0x80 != 0 { reader.run()? } else { 1 };
                        at = run(picture, at, length, *colours.get(usize::from(number & 0x7f)).ok_or(WRONG)?)?;
                    }
                }
                _ => return Err(WRONG),
            }
        }
    }
    Ok(())
}

/// The machine's side: writes an area of a picture as tiles, in each of the five ways.
#[cfg(any(test, feature = "stand-in"))]
pub mod stand_in {
    use super::*;

    /// The tiles of an area, not yet packed as a stream. Which way a tile is written
    /// follows from how many colours it has and from where it is, so that a picture
    /// with flat parts, slopes and noise is written in every way.
    pub fn tiles(picture: &Picture, area: Area) -> Vec<u8> {
        let mut written = Vec::new();
        let mut nth_tile = 0usize;
        for top in (area.top..area.top + area.height).step_by(TILE) {
            let high = TILE.min(area.top + area.height - top);
            for left in (area.left..area.left + area.width).step_by(TILE) {
                let wide = TILE.min(area.left + area.width - left);
                let pixels: Vec<[u8; 3]> = (0..wide * high)
                    .map(|nth| {
                        let at = ((top + nth / wide) * picture.width + left + nth % wide) * 4;
                        [picture.pixels[at], picture.pixels[at + 1], picture.pixels[at + 2]]
                    })
                    .collect();
                let mut colours: Vec<[u8; 3]> = Vec::new();
                for pixel in &pixels {
                    if colours.len() <= 127 && !colours.contains(pixel) {
                        colours.push(*pixel);
                    }
                }
                let number = |pixel: &[u8; 3]| colours.iter().position(|colour| colour == pixel).expect("a colour of the tile") as u8;
                // the runs of the tile: each a pixel and how often it follows itself
                let runs = || {
                    let mut runs: Vec<([u8; 3], usize)> = Vec::new();
                    for pixel in &pixels {
                        match runs.last_mut() {
                            Some((last, length)) if last == pixel => *length += 1,
                            _ => runs.push((*pixel, 1)),
                        }
                    }
                    runs
                };
                let length = |written: &mut Vec<u8>, mut length: usize| {
                    length -= 1;
                    while length >= 255 {
                        written.push(255);
                        length -= 255;
                    }
                    written.push(length as u8);
                };
                let odd = nth_tile % 2 == 1;
                nth_tile += 1;
                match colours.len() {
                    1 => {
                        written.push(1);
                        written.extend(colours[0]);
                    }
                    count @ 2..=16 if !odd => {
                        written.push(count as u8);
                        written.extend(colours.iter().flatten());
                        let bits = match count {
                            2 => 1,
                            3..=4 => 2,
                            _ => 4,
                        };
                        for row in pixels.chunks(wide) {
                            let mut packed = vec![0u8; (wide * bits).div_ceil(8)];
                            for (column, pixel) in row.iter().enumerate() {
                                let at = column * bits;
                                packed[at / 8] |= number(pixel) << (8 - bits - at % 8);
                            }
                            written.extend(packed);
                        }
                    }
                    count @ 2..=127 if !odd || count <= 16 => {
                        written.push(128 + count as u8);
                        written.extend(colours.iter().flatten());
                        for (pixel, run) in runs() {
                            match run {
                                1 => written.push(number(&pixel)),
                                _ => {
                                    written.push(number(&pixel) | 0x80);
                                    length(&mut written, run);
                                }
                            }
                        }
                    }
                    _ if odd => {
                        written.push(128);
                        for (pixel, run) in runs() {
                            written.extend(pixel);
                            length(&mut written, run);
                        }
                    }
                    _ => {
                        written.push(0);
                        written.extend(pixels.iter().flatten());
                    }
                }
            }
        }
        written
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture whose tiles are written in every way: one colour, two, five, twenty in
    /// long runs, and noise, with edges that are no multiple of a tile.
    fn of_every_kind() -> Picture {
        let mut picture = Picture::new(64 * 5 + 20, 64 * 2 + 9);
        let mut noise = 7u32;
        for top in 0..picture.height {
            for left in 0..picture.width {
                noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let pixel = match left / 64 {
                    0 => [9, 8, 7],
                    1 => [if (left + top) % 3 == 0 { 255 } else { 0 }; 3],
                    2 => [((left / 7 + top / 9) % 5 * 50) as u8, 1, 2],
                    3 => [(top / 4 % 20 * 12) as u8, 200, (left / 40) as u8],
                    4 => [(noise >> 8) as u8, (noise >> 16) as u8, (noise >> 24) as u8],
                    // a run longer than one byte holds: more than 255 pixels of one colour, then others
                    _ => [if top < 30 { 1 } else { (top % 3) as u8 }, 3, 3],
                };
                picture.set(left, top, pixel);
            }
        }
        picture
    }

    #[test]
    fn tiles_written_in_each_of_the_five_ways_are_read_back_as_they_were() {
        let picture = of_every_kind();
        let whole = Area { left: 0, top: 0, width: picture.width, height: picture.height };
        let written = stand_in::tiles(&picture, whole);
        // every way of writing a tile is among them
        let mut read = Picture::new(picture.width, picture.height);
        draw(&written, whole, &mut read).unwrap();
        assert!(read == picture, "the picture is the one that was written");
        // a part of the picture, at a place of its own
        let part = Area { left: 70, top: 5, width: 130, height: 100 };
        let mut read = Picture::new(picture.width, picture.height);
        draw(&stand_in::tiles(&picture, part), part, &mut read).unwrap();
        for top in 0..picture.height {
            for left in 0..picture.width {
                let at = (top * picture.width + left) * 4;
                let inside = (70..200).contains(&left) && (5..105).contains(&top);
                let expected = if inside { &picture.pixels[at..at + 3] } else { &[0, 0, 0][..] };
                assert_eq!(&read.pixels[at..at + 3], expected, "at {left}, {top}");
            }
        }
    }

    #[test]
    fn each_way_of_writing_a_tile_is_read_as_the_protocol_says() {
        let tile = |written: &[u8], wide: usize, high: usize| {
            let mut picture = Picture::new(wide, high);
            draw(written, Area { left: 0, top: 0, width: wide, height: high }, &mut picture)
                .map(|()| picture.pixels.as_chunks::<4>().0.iter().map(|pixel| pixel[0]).collect::<Vec<_>>())
        };
        // as it is; one colour
        assert_eq!(tile(&[0, 1, 0, 0, 2, 0, 0, 3, 0, 0], 3, 1), Ok(vec![1, 2, 3]));
        assert_eq!(tile(&[1, 9, 0, 0], 2, 2), Ok(vec![9, 9, 9, 9]));
        // two colours, a bit a pixel, each row on a byte of its own: 101, then 010
        assert_eq!(tile(&[2, 5, 0, 0, 6, 0, 0, 0b1010_0000, 0b0100_0000], 3, 2), Ok(vec![6, 5, 6, 5, 6, 5]));
        // three colours, two bits a pixel: 0, 1, 2, 2, 1
        assert_eq!(tile(&[3, 5, 0, 0, 6, 0, 0, 7, 0, 0, 0b0001_1010, 0b0100_0000], 5, 1), Ok(vec![5, 6, 7, 7, 6]));
        // five colours, four bits a pixel: 4, 0, 3
        let five = [5, 1, 0, 0, 2, 0, 0, 3, 0, 0, 4, 0, 0, 5, 0, 0, 0x40, 0x30];
        assert_eq!(tile(&five, 3, 1), Ok(vec![5, 1, 4]));
        // runs: three of one colour, then one of another; and a run of 256, as two bytes
        assert_eq!(tile(&[128, 4, 0, 0, 2, 8, 0, 0, 0], 4, 1), Ok(vec![4, 4, 4, 8]));
        assert_eq!(tile(&[128, 4, 0, 0, 255, 0], 64, 4).map(|tile| tile.len()), Ok(256));
        // colours and runs: the second colour three times, the first once, the second once
        assert_eq!(tile(&[130, 4, 0, 0, 8, 0, 0, 0x81, 2, 0, 1], 5, 1), Ok(vec![8, 8, 8, 4, 8]));

        // what is cut, runs past the tile, names a colour that is not there, or is of no kind, is refused
        for wrong in [
            &[0u8, 1, 0, 0][..],
            &[1, 9, 0],
            &[2, 5, 0, 0, 6, 0, 0],
            &[128, 4, 0, 0, 9],
            &[130, 4, 0, 0, 8, 0, 0, 2],
            &[130, 4, 0, 0, 8, 0, 0, 0x81],
            &[17],
            &[129],
            &[],
        ] {
            assert_eq!(tile(wrong, 2, 2), Err(WRONG), "{wrong:?}");
        }
    }
}
