//! The format a machine packs rectangles of 16 bits a pixel in (the interleaved runs of
//! MS-RDPBCGR 2.2.9.1.1.3.1.2.4, read as 3.1.9 says): a row is written as what it shares
//! with the row before it, which on a desktop is most of it.
//!
//! A Windows Server draws this way unless it is told otherwise: its desktops are capped
//! at 16 bits a pixel for every client that takes pictures as rectangles.
//!
//! The rows travel from the bottom of the rectangle up. Each piece says what kind it is
//! and how many pixels it stands for: pixels as in the row before, such pixels changed by
//! one colour, one colour repeated, two colours in turn, or pixels written out.

use crate::Error;

const WRONG: Error = Error::NotRdp("a rectangle's runs are not built as runs are");

/// A pixel of 16 bits as four bytes: blue, green, red, and a fourth that is always full.
/// Five and six bits become eight by repeating their highest, so that white stays white.
pub fn from_16_bits(pixel: u16) -> [u8; 4] {
    let (red, green, blue) = (pixel >> 11, (pixel >> 5) & 0x3f, pixel & 0x1f);
    [((blue << 3) | (blue >> 2)) as u8, ((green << 2) | (green >> 4)) as u8, ((red << 3) | (red >> 2)) as u8, 0xff]
}

/// A pixel of blue, green and red as a machine cuts it down to 16 bits: the highest five
/// bits of red and blue, the highest six of green.
pub fn to_16_bits(pixel: &[u8]) -> u16 {
    (u16::from(pixel[2] >> 3) << 11) | (u16::from(pixel[1] >> 2) << 5) | u16::from(pixel[0] >> 3)
}

/// Unpacks a rectangle of this many pixels. Gives its pixels from the top row down, four
/// bytes each: blue, green, red, and a fourth that is always full.
pub fn unpack(packed: &[u8], width: usize, height: usize) -> Result<Vec<u8>, Error> {
    let size = width.checked_mul(height).filter(|size| *size > 0).ok_or(WRONG)?;
    let pixels = pixels(packed, width, size)?;
    Ok(pixels.rchunks_exact(width).flatten().flat_map(|pixel| from_16_bits(*pixel)).collect())
}

/// What a piece stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Piece {
    /// Pixels as in the row before.
    AsBefore,
    /// Pixels as in the row before, each changed by the one colour; where the piece says
    /// so, it names that colour first.
    Changed { names_colour: bool },
    /// For each pixel one bit: as in the row before, or changed by the one colour.
    Either { names_colour: bool },
    /// One colour, repeated.
    Repeated,
    /// Two colours in turn.
    InTurn,
    /// Pixels written out.
    WrittenOut,
    /// Eight pixels of which these are changed by the one colour.
    EightOfWhich(u8),
    /// One pixel of this colour.
    One(u16),
}

fn byte(rest: &mut &[u8]) -> Result<u8, Error> {
    let (byte, after) = rest.split_first().ok_or(WRONG)?;
    *rest = after;
    Ok(*byte)
}

fn pixel(rest: &mut &[u8]) -> Result<u16, Error> {
    Ok(u16::from_le_bytes([byte(rest)?, byte(rest)?]))
}

/// Reads the head of a piece: what it stands for, and for how many pixels.
fn head(rest: &mut &[u8]) -> Result<(Piece, usize), Error> {
    let head = byte(rest)?;
    // a count too large for its bits follows in a byte of its own, less what the bits could say
    let mut count = |bits: u8, more_than: usize| -> Result<usize, Error> {
        match bits {
            0 => Ok(usize::from(byte(rest)?) + more_than),
            bits => Ok(usize::from(bits)),
        }
    };
    // where each pixel is one bit, the bits count whole bytes of them
    let in_bytes = |count: usize, bits: u8| if bits == 0 { count } else { count * 8 };
    Ok(match head {
        // the pieces that are one byte
        0xf9 => (Piece::EightOfWhich(0x03), 8),
        0xfa => (Piece::EightOfWhich(0x05), 8),
        0xfd => (Piece::One(0xffff), 1),
        0xfe => (Piece::One(0), 1),
        // the pieces with a count of two bytes
        0xf0..=0xf4 | 0xf6..=0xf8 => {
            let piece = match head {
                0xf0 => Piece::AsBefore,
                0xf1 => Piece::Changed { names_colour: false },
                0xf2 => Piece::Either { names_colour: false },
                0xf3 => Piece::Repeated,
                0xf4 => Piece::WrittenOut,
                0xf6 => Piece::Changed { names_colour: true },
                0xf7 => Piece::Either { names_colour: true },
                _ => Piece::InTurn,
            };
            (piece, usize::from(pixel(rest)?))
        }
        0xf5 | 0xfb | 0xfc | 0xff => return Err(WRONG),
        // the pieces with a count of four bits
        0xc0..=0xef => {
            let bits = head & 0x0f;
            match head >> 4 {
                0xc => (Piece::Changed { names_colour: true }, count(bits, 16)?),
                0xd => (Piece::Either { names_colour: true }, in_bytes(count(bits, 1)?, bits)),
                _ => (Piece::InTurn, count(bits, 16)?),
            }
        }
        // the pieces with a count of five bits
        _ => {
            let bits = head & 0x1f;
            match head >> 5 {
                0 => (Piece::AsBefore, count(bits, 32)?),
                1 => (Piece::Changed { names_colour: false }, count(bits, 32)?),
                2 => (Piece::Either { names_colour: false }, in_bytes(count(bits, 1)?, bits)),
                3 => (Piece::Repeated, count(bits, 32)?),
                4 => (Piece::WrittenOut, count(bits, 32)?),
                _ => return Err(WRONG),
            }
        }
    })
}

/// The pixels of a rectangle, as they travel: from the bottom row up.
fn pixels(packed: &[u8], width: usize, size: usize) -> Result<Vec<u16>, Error> {
    // no memory is taken for a size that is only said: it grows with what arrives
    let mut pixels: Vec<u16> = Vec::new();
    let mut rest = packed;
    // the one colour that pixels are changed by: white until a piece names another
    let mut colour = 0xffffu16;
    // the first row has no row before it: it counts as following a black one
    let mut first_row = true;
    // two pieces of pixels as before, one after the other, have one changed pixel between them
    let mut follows_as_before = false;
    while !rest.is_empty() {
        if first_row && pixels.len() >= width {
            first_row = false;
            follows_as_before = false;
        }
        let (piece, count) = head(&mut rest)?;
        if pixels.len() + count * if piece == Piece::InTurn { 2 } else { 1 } > size {
            return Err(Error::NotRdp("a rectangle's runs hold more than its size"));
        }
        let before = |pixels: &Vec<u16>| if first_row { 0 } else { pixels[pixels.len() - width] };
        let between = std::mem::replace(&mut follows_as_before, piece == Piece::AsBefore);
        match piece {
            Piece::AsBefore => {
                for nth in 0..count {
                    let pixel = before(&pixels) ^ if nth == 0 && between { colour } else { 0 };
                    pixels.push(pixel);
                }
            }
            Piece::Changed { names_colour } => {
                if names_colour {
                    colour = pixel(&mut rest)?;
                }
                for _ in 0..count {
                    let pixel = before(&pixels) ^ colour;
                    pixels.push(pixel);
                }
            }
            Piece::Either { names_colour } => {
                if names_colour {
                    colour = pixel(&mut rest)?;
                }
                let mut left = count;
                while left > 0 {
                    let which = byte(&mut rest)?;
                    for bit in 0..left.min(8) {
                        let pixel = before(&pixels) ^ if which >> bit & 1 == 1 { colour } else { 0 };
                        pixels.push(pixel);
                    }
                    left -= left.min(8);
                }
            }
            Piece::EightOfWhich(which) => {
                for bit in 0..8 {
                    let pixel = before(&pixels) ^ if which >> bit & 1 == 1 { colour } else { 0 };
                    pixels.push(pixel);
                }
            }
            Piece::Repeated => {
                let repeated = pixel(&mut rest)?;
                pixels.extend(std::iter::repeat_n(repeated, count));
            }
            Piece::InTurn => {
                let (first, second) = (pixel(&mut rest)?, pixel(&mut rest)?);
                for _ in 0..count {
                    pixels.extend([first, second]);
                }
            }
            Piece::WrittenOut => {
                for _ in 0..count {
                    pixels.push(pixel(&mut rest)?);
                }
            }
            Piece::One(one) => pixels.push(one),
        }
    }
    if pixels.len() != size {
        return Err(Error::NotRdp("a rectangle's runs hold less than its size"));
    }
    Ok(pixels)
}

/// Packs pixels the way a machine does at 16 bits a pixel, for the stand-in of the
/// tests. The pixels come as they travel: from the bottom row up. Every kind of piece is
/// written where the pixels allow it, in each of the lengths a count is written in.
#[cfg(any(test, feature = "stand-in"))]
pub fn pack(pixels: &[u16], width: usize) -> Vec<u8> {
    // a piece with its count: in the bits of its head, in a byte of its own, or in two
    fn piece(packed: &mut Vec<u8>, piece: Piece, count: usize) {
        let (short, long, bits, more_than): (u8, u8, u32, usize) = match piece {
            Piece::AsBefore => (0x00, 0xf0, 5, 32),
            Piece::Changed { names_colour: false } => (0x20, 0xf1, 5, 32),
            Piece::Either { names_colour: false } => (0x40, 0xf2, 5, 1),
            Piece::Repeated => (0x60, 0xf3, 5, 32),
            Piece::WrittenOut => (0x80, 0xf4, 5, 32),
            Piece::Changed { names_colour: true } => (0xc0, 0xf6, 4, 16),
            Piece::Either { names_colour: true } => (0xd0, 0xf7, 4, 1),
            Piece::InTurn => (0xe0, 0xf8, 4, 16),
            Piece::EightOfWhich(0x03) => return packed.push(0xf9),
            Piece::EightOfWhich(_) => return packed.push(0xfa),
            Piece::One(0xffff) => return packed.push(0xfd),
            Piece::One(_) => return packed.push(0xfe),
        };
        let most = (1usize << bits) - 1;
        let in_bits = match piece {
            Piece::Either { .. } => (count.is_multiple_of(8) && count / 8 <= most).then_some(count / 8),
            _ => (count <= most).then_some(count),
        };
        match in_bits {
            Some(in_bits) => packed.push(short | in_bits as u8),
            None if (more_than..more_than + 256).contains(&count) => packed.extend([short, (count - more_than) as u8]),
            None => {
                packed.push(long);
                packed.extend((count as u16).to_le_bytes());
            }
        }
    }

    let mut packed = Vec::new();
    let mut colour = 0xffffu16;
    let (mut at, mut first_row, mut follows_as_before) = (0, true, false);
    while at < pixels.len() {
        if first_row && at >= width {
            first_row = false;
            follows_as_before = false;
        }
        // a piece of the first row ends with it, and no piece stands for more than its count can say
        let most = if first_row { width - at } else { (pixels.len() - at).min(usize::from(u16::MAX)) };
        let before = |at: usize| if first_row { 0 } else { pixels[at - width] };
        let run = |from: usize, most: usize, holds: &dyn Fn(usize) -> bool| (from..from + most).take_while(|at| holds(*at)).count();
        let changed_by = pixels[at] ^ before(at);
        let between = std::mem::replace(&mut follows_as_before, false);

        let as_before = run(at, most, &|at| pixels[at] == before(at));
        // after a piece of pixels as before, another one begins with a pixel changed by the one colour
        let as_before = match between {
            false => as_before,
            true if changed_by == colour => 1 + run(at + 1, most - 1, &|at| pixels[at] == before(at)),
            true => 0,
        };
        let changed = run(at, most, &|at| pixels[at] == before(at) ^ changed_by);
        let repeated = run(at, most, &|next| pixels[next] == pixels[at]);
        let in_turn =
            (0..most / 2).take_while(|pair| pixels[at + pair * 2] == pixels[at] && pixels[at + pair * 2 + 1] == pixels[at + 1]).count();
        let either = run(at, most, &|at| pixels[at] == before(at) || pixels[at] == before(at) ^ changed_by);
        let which = |from: usize| (0..8).fold(0u8, |which, bit| which | (u8::from(pixels[from + bit] != before(from + bit)) << bit));

        let (written, stands_for) = if as_before > 0 && (!between || as_before > 1) {
            follows_as_before = true;
            (Piece::AsBefore, as_before)
        } else if changed >= 3 {
            let names_colour = changed_by != colour;
            colour = changed_by;
            (Piece::Changed { names_colour }, changed)
        } else if repeated >= 3 {
            (Piece::Repeated, repeated)
        } else if in_turn >= 2 && pixels[at] != pixels[at + 1] {
            (Piece::InTurn, in_turn)
        } else if either >= 8 && changed_by == colour && matches!(which(at), 0x03 | 0x05) {
            (Piece::EightOfWhich(which(at)), 8)
        } else if either >= 6 {
            let names_colour = changed_by != colour;
            colour = changed_by;
            (Piece::Either { names_colour }, either)
        } else if matches!(pixels[at], 0 | 0xffff) {
            (Piece::One(pixels[at]), 1)
        } else {
            // written out, until something shorter begins
            let shorter = |at: usize| {
                pixels[at] == before(at) || (at + 2 < pixels.len() && pixels[at] == pixels[at + 1] && pixels[at] == pixels[at + 2])
            };
            (Piece::WrittenOut, 1 + run(at + 1, most - 1, &|at| !shorter(at)))
        };

        piece(&mut packed, written, stands_for);
        match written {
            Piece::Changed { names_colour: true } => packed.extend(colour.to_le_bytes()),
            Piece::Either { names_colour } => {
                if names_colour {
                    packed.extend(colour.to_le_bytes());
                }
                for from in (at..at + stands_for).step_by(8) {
                    let bits = (at + stands_for - from).min(8);
                    packed.push((0..bits).fold(0u8, |which, bit| which | (u8::from(pixels[from + bit] != before(from + bit)) << bit)));
                }
            }
            Piece::Repeated => packed.extend(pixels[at].to_le_bytes()),
            Piece::InTurn => packed.extend([pixels[at].to_le_bytes(), pixels[at + 1].to_le_bytes()].concat()),
            Piece::WrittenOut => packed.extend(pixels[at..at + stands_for].iter().flat_map(|pixel| pixel.to_le_bytes())),
            _ => {}
        }
        at += stands_for * if written == Piece::InTurn { 2 } else { 1 };
    }
    packed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unpacked(packed: &[u8], width: usize, height: usize) -> Result<Vec<u16>, Error> {
        pixels(packed, width, width * height)
    }

    #[test]
    fn the_first_row_follows_a_black_one() {
        // pixels as before: black. Then changed by the one colour, which is white until one is named
        assert_eq!(unpacked(&[0x03, 0x22], 5, 1), Ok(vec![0, 0, 0, 0xffff, 0xffff]));
        // a piece that names its colour, and one that keeps it
        assert_eq!(unpacked(&[0xc2, 0x34, 0x12, 0x01, 0x21], 4, 1), Ok(vec![0x1234, 0x1234, 0, 0x1234]));
        // for each pixel one bit, lowest first: five pixels in one byte of bits
        assert_eq!(unpacked(&[0x40, 4, 0b10110], 5, 1), Ok(vec![0, 0xffff, 0xffff, 0, 0xffff]));
    }

    #[test]
    fn a_later_row_is_written_as_what_it_shares_with_the_row_before() {
        // the first row written out, the second as before, the third changed by a colour
        let packed = [0x83, 1, 0, 2, 0, 3, 0, /* as before */ 0x03, /* changed by 0x00f0 */ 0xc3, 0xf0, 0x00];
        assert_eq!(unpacked(&packed, 3, 3), Ok(vec![1, 2, 3, 1, 2, 3, 0xf1, 0xf2, 0xf3]));
        // bits in a later row: as before, or changed
        let packed = [0x83, 1, 0, 2, 0, 3, 0, /* names 0x0100, three bits */ 0xd0, 2, 0x00, 0x01, 0b101];
        assert_eq!(unpacked(&packed, 3, 2), Ok(vec![1, 2, 3, 0x101, 2, 0x103]));
        // the two pieces of eight pixels with their bits built in
        let first: Vec<u8> = [0x88].into_iter().chain((1..=8u16).flat_map(u16::to_le_bytes)).collect();
        assert_eq!(unpacked(&[&first[..], &[0xf9]].concat(), 8, 2).unwrap()[8..], [!1, !2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(unpacked(&[&first[..], &[0xfa]].concat(), 8, 2).unwrap()[8..], [!1, 2, !3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn two_pieces_of_pixels_as_before_have_one_changed_pixel_between_them() {
        let packed = [0x83, 1, 0, 2, 0, 3, 0, /* one as before */ 0x01, /* and two more: the first of them changed */ 0x02];
        assert_eq!(unpacked(&packed, 3, 2), Ok(vec![1, 2, 3, 1, !2, 3]));
        // in the first row the pixel between them is the one colour itself
        assert_eq!(unpacked(&[0x01, 0x02], 3, 1), Ok(vec![0, 0xffff, 0]));
        // another piece in between, and nothing is changed
        assert_eq!(unpacked(&[0x01, 0xfe, 0x01], 3, 1), Ok(vec![0, 0, 0]));
        // and what was so in the first row is forgotten where the second begins
        assert_eq!(unpacked(&[0x02, 0x02], 2, 2), Ok(vec![0, 0, 0, 0]));
    }

    #[test]
    fn colours_are_repeated_taken_in_turn_and_written_out() {
        assert_eq!(unpacked(&[0x63, 0xcd, 0xab], 3, 1), Ok(vec![0xabcd; 3]));
        assert_eq!(unpacked(&[0xe2, 1, 0, 2, 0], 4, 1), Ok(vec![1, 2, 1, 2]));
        assert_eq!(unpacked(&[0x82, 1, 0, 2, 0, 0xfd, 0xfe], 4, 1), Ok(vec![1, 2, 0xffff, 0]));
    }

    #[test]
    fn a_count_is_written_in_its_head_in_a_byte_or_in_two() {
        // 31 in the head, 32 and 287 in a byte of their own, 288 in two
        for (packed, count) in
            [(vec![0x7f, 7, 0], 31), (vec![0x60, 0, 7, 0], 32), (vec![0x60, 255, 7, 0], 287), (vec![0xf3, 0x20, 0x01, 7, 0], 288)]
        {
            assert_eq!(unpacked(&packed, count, 1), Ok(vec![7; count]), "{count}");
        }
        // with a count of four bits: 15, then 16 and 271, then two bytes
        for (packed, count) in
            [(vec![0xcf, 7, 0], 15), (vec![0xc0, 0, 7, 0], 16), (vec![0xc0, 255, 7, 0], 271), (vec![0xf6, 0x10, 0x01, 7, 0], 272)]
        {
            assert_eq!(unpacked(&packed, count, 1), Ok(vec![7; count]), "{count}");
        }
        // bits: whole bytes of them in the head, any count in a byte of its own, or in two
        assert_eq!(unpacked(&[0x42, 0xff, 0x0f], 16, 1).unwrap().iter().filter(|pixel| **pixel != 0).count(), 12);
        assert_eq!(unpacked(&[0x40, 8, 0xff, 0x01], 9, 1), Ok(vec![0xffff; 9]));
        assert_eq!(unpacked(&[0xf2, 9, 0, 0xff, 0x01], 9, 1), Ok(vec![0xffff; 9]));
        assert_eq!(unpacked(&[0xf7, 9, 0, 5, 0, 0xff, 0x01], 9, 1), Ok(vec![5; 9]));
        // pixels as before, and two colours in turn, in two bytes
        assert_eq!(unpacked(&[0xf0, 4, 0], 4, 1), Ok(vec![0; 4]));
        assert_eq!(unpacked(&[0xf8, 2, 0, 1, 0, 2, 0], 4, 1), Ok(vec![1, 2, 1, 2]));
        assert_eq!(unpacked(&[0xf1, 2, 0, 0xf4, 2, 0, 1, 0, 2, 0], 4, 1), Ok(vec![0xffff, 0xffff, 1, 2]));
    }

    #[test]
    fn a_piece_that_begins_in_the_first_row_ends_as_it_began() {
        // four pixels as before from the first row on: black, into the second row as well
        assert_eq!(unpacked(&[0x62, 9, 0, 0x04], 3, 2), Ok(vec![9, 9, 0, 0, 0, 0]));
        // the same begun in the second row takes the row before
        assert_eq!(unpacked(&[0x63, 9, 0, 0x03], 3, 2), Ok(vec![9, 9, 9, 9, 9, 9]));
    }

    #[test]
    fn what_holds_more_or_less_than_its_size_is_refused_and_so_is_what_is_cut() {
        assert_eq!(unpacked(&[0x63, 9, 0], 2, 1), Err(Error::NotRdp("a rectangle's runs hold more than its size")));
        assert_eq!(unpacked(&[0xf8, 0xff, 0xff, 1, 0, 2, 0], 4, 4), Err(Error::NotRdp("a rectangle's runs hold more than its size")));
        assert_eq!(unpacked(&[0x62, 9, 0], 3, 1), Err(Error::NotRdp("a rectangle's runs hold less than its size")));
        assert_eq!(unpacked(&[], 3, 1), Err(Error::NotRdp("a rectangle's runs hold less than its size")));
        for cut in [&[0x63, 9][..], &[0x60], &[0xf3, 3], &[0x83, 1, 0, 2], &[0xc2, 1], &[0x40, 9, 0xff], &[0xe1, 1, 0, 2]] {
            assert_eq!(unpacked(cut, 16, 16), Err(WRONG), "{cut:?}");
        }
        for unknown in [0xa1u8, 0xbf, 0xf5, 0xfb, 0xfc, 0xff] {
            assert_eq!(unpacked(&[unknown, 0, 0, 0, 0], 4, 1), Err(WRONG), "{unknown:#x}");
        }
        assert!(unpack(&[0x61, 0, 0], 0, 1).is_err());
        assert!(unpack(&[0x61, 0, 0], usize::MAX, 2).is_err());
    }

    #[test]
    fn colours_of_16_bits_become_colours_of_24_and_white_stays_white() {
        assert_eq!(from_16_bits(0xffff), [255, 255, 255, 255]);
        assert_eq!(from_16_bits(0), [0, 0, 0, 255]);
        assert_eq!(from_16_bits(0xf800), [0, 0, 255, 255], "red is the highest five bits");
        assert_eq!(from_16_bits(0x07e0), [0, 255, 0, 255]);
        assert_eq!(from_16_bits(0x001f), [255, 0, 0, 255]);
        // cut down and brought back, a colour keeps the bits that travel
        for pixel in [[0x12u8, 0x34, 0x56], [0xff, 0x00, 0x80], [7, 3, 7], [0xf8, 0xfc, 0xf8]] {
            let there = to_16_bits(&pixel);
            let back = from_16_bits(there);
            assert_eq!(to_16_bits(&back), there);
            assert_eq!((back[0] >> 3, back[1] >> 2, back[2] >> 3), (pixel[0] >> 3, pixel[1] >> 2, pixel[2] >> 3));
        }
        // rows come out from the top down
        assert_eq!(unpack(&[0x82, 0x1f, 0, 0xe0, 0x07], 1, 2).unwrap(), [0, 255, 0, 255, 255, 0, 0, 255]);
    }

    /// The pieces that packed bytes are made of.
    fn pieces(packed: &[u8]) -> Vec<Piece> {
        let mut pieces = Vec::new();
        let mut rest = packed;
        while !rest.is_empty() {
            let (piece, count) = head(&mut rest).unwrap();
            // what follows the head: the colours it names, its bits, its pixels
            let follows = match piece {
                Piece::Changed { names_colour: true } | Piece::Repeated => 2,
                Piece::Either { names_colour } => usize::from(names_colour) * 2 + count.div_ceil(8),
                Piece::InTurn => 4,
                Piece::WrittenOut => count * 2,
                _ => 0,
            };
            rest = &rest[follows..];
            pieces.push(piece);
        }
        pieces
    }

    /// Pixels that hold flat parts, rows that repeat, stripes, slopes and noise.
    fn desktop(width: usize, height: usize, seed: u32) -> Vec<u16> {
        let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
        let mut pixels = Vec::with_capacity(width * height);
        for row in 0..height {
            for column in 0..width {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                pixels.push(match (column * 8 / width.max(1) + seed as usize) % 8 {
                    0 => 0x7bef,
                    1 => (column as u16).wrapping_mul(0x0841),
                    2 => (state >> 16) as u16,
                    3 => {
                        if row % 2 == 0 {
                            0
                        } else {
                            0xffff
                        }
                    }
                    4 => {
                        if column % 2 == 0 {
                            0x1234
                        } else {
                            0x4321
                        }
                    }
                    5 => {
                        if (state >> 20).is_multiple_of(3) {
                            0x00ff
                        } else {
                            0x7bef
                        }
                    }
                    6 => {
                        if (row / 3) % 2 == 0 {
                            0xf800
                        } else {
                            (state >> 28) as u16
                        }
                    }
                    _ => (row as u16) << 5,
                });
            }
        }
        pixels
    }

    #[test]
    fn what_is_packed_unpacks_to_the_same_pixels() {
        let mut kinds = Vec::new();
        for (width, height) in [(1, 1), (1, 9), (2, 2), (3, 5), (8, 8), (16, 4), (17, 3), (64, 64), (100, 7), (131, 33), (300, 3), (640, 2)]
        {
            for seed in 0..8 {
                let pixels = desktop(width, height, seed);
                let packed = pack(&pixels, width);
                assert_eq!(unpacked(&packed, width, height).as_ref(), Ok(&pixels), "{width} by {height}, seed {seed}");
                for piece in pieces(&packed) {
                    if !kinds.contains(&std::mem::discriminant(&piece)) {
                        kinds.push(std::mem::discriminant(&piece));
                    }
                }
            }
        }
        // eight pixels of which the first two, or the first and the third, are changed by the one colour
        let first: Vec<u16> = (1..=8).collect();
        let second: Vec<u16> = first.iter().map(|pixel| pixel ^ 0x0f00).collect();
        let third: Vec<u16> = second.iter().enumerate().map(|(at, pixel)| if at < 2 { pixel ^ 0x0f00 } else { *pixel }).collect();
        let fourth: Vec<u16> =
            third.iter().enumerate().map(|(at, pixel)| if at % 2 == 0 && at < 3 { pixel ^ 0x0f00 } else { *pixel }).collect();
        // the second row twice, so that the row before those eight ends a piece of pixels as before
        let eights = [first, second.clone(), second, third, fourth].concat();
        let packed = pack(&eights, 8);
        assert_eq!(unpacked(&packed, 8, 5), Ok(eights));
        assert_eq!(pieces(&packed)[3..], [Piece::EightOfWhich(0x03), Piece::EightOfWhich(0x05)]);
        if !kinds.contains(&std::mem::discriminant(&Piece::EightOfWhich(0))) {
            kinds.push(std::mem::discriminant(&Piece::EightOfWhich(0)));
        }
        assert_eq!(kinds.len(), 8, "every kind of piece is written");
        // one colour all over: a few bytes for the whole rectangle
        let flat = vec![0x7bef; 64 * 64];
        assert!(pack(&flat, 64).len() < 16);
        assert_eq!(unpacked(&pack(&flat, 64), 64, 64), Ok(flat));
    }
}
