//! The format a machine packs 32-bit rectangles in when nothing may be left out (the
//! planar format of MS-RDPEGDI 2.2.2.5.1): each colour as a plane of its own, and each
//! plane either as it is or as runs.
//!
//! A plane's rows travel from the bottom of the rectangle up. The first row holds the
//! values themselves, and every later row what changed against the row before it.

use crate::Error;

const WRONG: Error = Error::NotRdp("a rectangle's planes are not built as planes are");

/// Unpacks a rectangle of this many pixels. Gives its pixels from the top row down, four
/// bytes each: blue, green, red, and a fourth that is always full.
pub fn unpack(packed: &[u8], width: usize, height: usize) -> Result<Vec<u8>, Error> {
    let (head, mut rest) = packed.split_first().ok_or(WRONG)?;
    // colours left out, or planes of half the size: Away asks for neither
    if head & 0x0f != 0 {
        return Err(Error::NotYet("leaves colours out of what it draws"));
    }
    let (in_runs, without_alpha) = (head & 0x10 != 0, head & 0x20 != 0);
    let size = width.checked_mul(height).filter(|size| *size > 0).ok_or(WRONG)?;
    let plane = |rest: &mut &[u8]| -> Result<Vec<u8>, Error> {
        if in_runs {
            runs(rest, width, height)
        } else {
            let (plane, after) = rest.split_at_checked(size).ok_or(WRONG)?;
            *rest = after;
            Ok(plane.to_vec())
        }
    };
    if !without_alpha {
        plane(&mut rest)?;
    }
    let (red, green, blue) = (plane(&mut rest)?, plane(&mut rest)?, plane(&mut rest)?);
    let mut pixels = Vec::with_capacity(size * 4);
    for row in (0..height).rev() {
        for at in row * width..(row + 1) * width {
            pixels.extend_from_slice(&[blue[at], green[at], red[at], 0xff]);
        }
    }
    Ok(pixels)
}

/// One plane that travels as runs: for each row, pieces of values written out, each
/// followed by a run of the last of them.
fn runs(rest: &mut &[u8], width: usize, height: usize) -> Result<Vec<u8>, Error> {
    let mut plane = vec![0u8; width * height];
    let mut take = || -> Result<u8, Error> {
        let (byte, after) = rest.split_first().ok_or(WRONG)?;
        *rest = after;
        Ok(*byte)
    };
    for row in 0..height {
        let mut at = 0;
        // the last value written out: the value itself in the first row, a change after it
        let mut last = 0i16;
        while at < width {
            let control = take()?;
            let (mut run, mut written) = (usize::from(control & 0x0f), usize::from(control >> 4));
            // a run too long for four bits borrows the other four
            if run == 1 {
                (run, written) = (written + 16, 0);
            } else if run == 2 {
                (run, written) = (written + 32, 0);
            }
            if run + written == 0 || at + run + written > width {
                return Err(WRONG);
            }
            for nth in 0..written + run {
                if nth < written {
                    let byte = take()?;
                    last = match (row, byte & 1) {
                        (0, _) => i16::from(byte),
                        (_, 0) => i16::from(byte >> 1),
                        (_, _) => -i16::from(byte >> 1) - 1,
                    };
                }
                plane[row * width + at] = if row == 0 { last as u8 } else { (i16::from(plane[(row - 1) * width + at]) + last) as u8 };
                at += 1;
            }
        }
    }
    Ok(plane)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planes_that_travel_as_they_are_are_turned_the_right_way_up() {
        // two by two, no alpha: red, green, blue, each from the bottom row up, then one spare byte
        let packed = [0x20, 1, 2, 3, 4, 10, 20, 30, 40, 100, 110, 120, 130, 0];
        assert_eq!(
            unpack(&packed, 2, 2).unwrap(),
            [120, 30, 3, 255, 130, 40, 4, 255, /* the top row, which travelled last */ 100, 10, 1, 255, 110, 20, 2, 255]
        );
    }

    #[test]
    fn an_alpha_plane_is_read_and_left_out() {
        let packed = [0x00, 9, 9, 1, 2, 10, 20, 100, 110, 0];
        assert_eq!(unpack(&packed, 2, 1).unwrap(), [100, 10, 1, 255, 110, 20, 2, 255]);
    }

    #[test]
    fn a_first_row_is_values_written_out_and_runs_of_the_last() {
        // six wide: two values written out, then a run of four of the second
        let mut rest: &[u8] = &[0x24, 7, 9];
        assert_eq!(runs(&mut rest, 6, 1).unwrap(), [7, 9, 9, 9, 9, 9]);
        assert!(rest.is_empty());
        // a run at the start of a row, with nothing written out, is a run of zeros
        let mut rest: &[u8] = &[0x06];
        assert_eq!(runs(&mut rest, 6, 1).unwrap(), [0; 6]);
    }

    #[test]
    fn long_runs_borrow_the_other_four_bits() {
        // 20 wide: a run of 16 + 3, after one value written out
        let mut rest: &[u8] = &[0x10, 5, 0x31];
        assert_eq!(runs(&mut rest, 20, 1).unwrap(), [5; 20]);
        // 40 wide: one written out, then a run of 32 + 7
        let mut rest: &[u8] = &[0x10, 5, 0x72];
        assert_eq!(runs(&mut rest, 40, 1).unwrap(), [5; 40]);
    }

    #[test]
    fn later_rows_are_changes_against_the_row_before() {
        // three wide, three rows: 10 20 30, then +1 -1 0, then the same again as a run of no change
        let mut rest: &[u8] = &[0x30, 10, 20, 30, 0x30, 2, 1, 0, 0x03];
        assert_eq!(runs(&mut rest, 3, 3).unwrap(), [10, 20, 30, 11, 19, 30, 11, 19, 30]);
        // a change that runs: every pixel two more than the one above it
        let mut rest: &[u8] = &[0x13, 100, 0x13, 4];
        assert_eq!(runs(&mut rest, 4, 2).unwrap(), [100, 100, 100, 100, 102, 102, 102, 102]);
        // changes wrap around, as bytes do
        let mut rest: &[u8] = &[0x10, 250, 0x10, 20];
        assert_eq!(runs(&mut rest, 1, 2).unwrap(), [250, 4]);
    }

    #[test]
    fn what_is_cut_or_too_long_for_its_row_is_refused() {
        // four wide, two rows, each plane: one value and a run of three, then a row of no change
        let whole = [0x30u8, 0x13, 7, 0x04, 0x13, 8, 0x04, 0x13, 9, 0x04];
        assert_eq!(unpack(&whole, 4, 2).unwrap(), [9, 8, 7, 255].repeat(8));
        for cut in 0..whole.len() {
            assert!(unpack(&whole[..cut], 4, 2).is_err(), "cut at {cut}");
        }
        assert!(runs(&mut &[0x24u8, 7, 9][..], 5, 1).is_err(), "longer than its row");
        assert!(runs(&mut &[0x00u8][..], 5, 1).is_err(), "a piece of nothing");
        assert_eq!(unpack(&[0x31], 1, 1), Err(Error::NotYet("leaves colours out of what it draws")));
        assert!(unpack(&[0x20], 0, 5).is_err());
    }
}
