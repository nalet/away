//! A desktop's picture as it is on the machine: every rectangle the machine draws is put
//! where it belongs.

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

/// A rectangle of a picture: where it is, and how large.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    /// Its left edge.
    pub left: usize,
    /// Its top edge.
    pub top: usize,
    /// How wide it is.
    pub width: usize,
    /// How high.
    pub height: usize,
}

impl Picture {
    /// A black picture of this size.
    pub fn new(width: usize, height: usize) -> Self {
        let mut pixels = vec![0; width * height * 4];
        pixels.as_chunks_mut::<4>().0.iter_mut().for_each(|pixel| pixel[3] = 0xff);
        Picture { width, height, pixels }
    }

    /// Whether an area lies inside the picture.
    pub fn holds(&self, area: Area) -> bool {
        area.left + area.width <= self.width && area.top + area.height <= self.height
    }

    /// Sets one pixel: blue, green, red. The place is inside the picture.
    pub fn set(&mut self, left: usize, top: usize, pixel: [u8; 3]) {
        let at = (top * self.width + left) * 4;
        self.pixels[at..at + 3].copy_from_slice(&pixel);
    }

    /// Fills an area that lies inside the picture with one colour.
    pub fn fill(&mut self, area: Area, pixel: [u8; 3]) {
        for row in area.top..area.top + area.height {
            let from = (row * self.width + area.left) * 4;
            for to in self.pixels[from..from + area.width * 4].as_chunks_mut::<4>().0 {
                to[..3].copy_from_slice(&pixel);
            }
        }
    }

    /// Copies what is at one place of the picture to an area of it, both inside the
    /// picture. The two may lie over each other.
    pub fn copy(&mut self, from: (usize, usize), to: Area) {
        let row = to.width * 4;
        let rows: Vec<usize> = match from.1 < to.top {
            // downwards: the lowest row first, so that no row is read after it was written
            true => (0..to.height).rev().collect(),
            false => (0..to.height).collect(),
        };
        for nth in rows {
            let read = ((from.1 + nth) * self.width + from.0) * 4;
            let write = ((to.top + nth) * self.width + to.left) * 4;
            self.pixels.copy_within(read..read + row, write);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_part_is_copied_onto_itself_whichever_way_it_moves() {
        let numbered = || {
            let mut picture = Picture::new(4, 4);
            for at in 0..16 {
                picture.set(at % 4, at / 4, [at as u8, 0, 0]);
            }
            picture
        };
        let blues = |picture: &Picture| picture.pixels.as_chunks::<4>().0.iter().map(|pixel| pixel[0]).collect::<Vec<_>>();
        // down by one row
        let mut picture = numbered();
        picture.copy((0, 0), Area { left: 0, top: 1, width: 4, height: 3 });
        assert_eq!(blues(&picture), [0, 1, 2, 3, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
        // up by one row, and to the right by one within a row
        let mut picture = numbered();
        picture.copy((0, 1), Area { left: 0, top: 0, width: 4, height: 3 });
        assert_eq!(blues(&picture), [4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 12, 13, 14, 15]);
        let mut picture = numbered();
        picture.copy((0, 0), Area { left: 1, top: 0, width: 3, height: 1 });
        assert_eq!(blues(&picture)[..4], [0, 0, 1, 2]);
        assert!(
            picture.holds(Area { left: 1, top: 1, width: 3, height: 3 }) && !picture.holds(Area { left: 2, top: 0, width: 3, height: 1 })
        );
        // the fourth byte of every pixel stays full
        assert!(picture.pixels.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 0xff));
    }
}
