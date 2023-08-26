use crate::color::Colorize;
use std::ops::Range;
pub struct BufferDiffIter<'a, T: PartialEq + Clone> {
    current: &'a [T],
    prev: &'a [T],
    idx: usize,
}

impl<'a, T: PartialEq + Clone> BufferDiffIter<'a, T> {
    pub fn new(current: &'a [T], prev: &'a [T]) -> Self {
        assert_eq!(
            prev.len(),
            current.len(),
            "both current and prev must be the same length"
        );
        Self {
            current,
            prev,
            idx: 0,
        }
    }
}

impl<'a, T: PartialEq + Clone> Iterator for BufferDiffIter<'a, T> {
    type Item = (Range<usize>, T);
    fn next(&mut self) -> Option<Self::Item> {
        while self.prev.get(self.idx)? == self.current.get(self.idx)? {
            self.idx += 1;
        }
        let start = self.idx;
        let item = self.current.get(self.idx)?;
        loop {
            match self.current.get(self.idx) {
                Some(i) if i == item && i != &self.prev[self.idx] => self.idx += 1,
                _ => return Some((start..self.idx, item.clone())),
            }
        }
    }
}

// Technically this is unneeded lmfao. This used to contain a pixel sorter, but then benchmarks showed it was too slow
pub struct Differ<C: Colorize> {
    data: Vec<(Range<usize>, C, u8)>,
}

impl<C: Colorize> Differ<C> {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            data: Vec::with_capacity(width as usize * height as usize),
        }
    }
    pub fn assign_diff(&mut self, curr: &[[u8; 4]], prev: &[[u8; 4]]) {
        self.data.clear();
        let diff_iter = BufferDiffIter::new(curr, prev)
            .map(|(pos, [r, g, b, chr])| (pos, C::from_rgb([r, g, b]), chr));

        self.data.extend(diff_iter);
    }
    pub fn data(&self) -> &[(Range<usize>, C, u8)] {
        &self.data
    }
}
