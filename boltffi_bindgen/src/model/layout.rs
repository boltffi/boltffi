use std::ops::Add;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Offset(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Alignment(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub size: Size,
    pub alignment: Alignment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldLayout {
    pub offset: Offset,
    pub size: Size,
    pub alignment: Alignment,
}

impl Offset {
    pub const ZERO: Self = Self(0);

    pub fn aligned_to(self, alignment: Alignment) -> Self {
        Self((self.0 + alignment.0 - 1) & !(alignment.0 - 1))
    }

    pub fn as_usize(self) -> usize {
        self.0
    }
}

impl Add<Size> for Offset {
    type Output = Self;

    fn add(self, size: Size) -> Self {
        Self(self.0 + size.0)
    }
}

impl Size {
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    pub fn as_usize(self) -> usize {
        self.0
    }

    pub fn padded_to(self, alignment: Alignment) -> Self {
        Self((self.0 + alignment.0 - 1) & !(alignment.0 - 1))
    }
}

impl Alignment {
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    pub fn max(self, other: Self) -> Self {
        Self(self.0.max(other.0))
    }

    pub fn as_usize(self) -> usize {
        self.0
    }
}

impl Layout {
    pub const fn new(size: usize, alignment: usize) -> Self {
        Self {
            size: Size(size),
            alignment: Alignment(alignment),
        }
    }
}

impl FieldLayout {
    pub fn new(offset: Offset, layout: Layout) -> Self {
        Self {
            offset,
            size: layout.size,
            alignment: layout.alignment,
        }
    }
}

pub trait CLayout {
    fn c_layout(&self) -> Layout;
}

pub struct StructLayout {
    fields: Vec<FieldLayout>,
    total_size: Size,
    max_alignment: Alignment,
}

impl StructLayout {
    pub fn from_layouts<I>(layouts: I) -> Self
    where
        I: IntoIterator<Item = Layout>,
    {
        let (fields, final_offset, max_alignment) = layouts
            .into_iter()
            .fold((Vec::new(), Offset::ZERO, Alignment::new(1)), |(mut fields, offset, max_align), layout| {
                let aligned_offset = offset.aligned_to(layout.alignment);
                fields.push(FieldLayout::new(aligned_offset, layout));
                (fields, aligned_offset + layout.size, max_align.max(layout.alignment))
            });

        let total_size = Size::new(final_offset.as_usize()).padded_to(max_alignment);

        Self {
            fields,
            total_size,
            max_alignment,
        }
    }

    pub fn fields(&self) -> &[FieldLayout] {
        &self.fields
    }

    pub fn total_size(&self) -> Size {
        self.total_size
    }

    pub fn alignment(&self) -> Alignment {
        self.max_alignment
    }

    pub fn offsets(&self) -> impl Iterator<Item = Offset> + '_ {
        self.fields.iter().map(|field| field.offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_alignment() {
        let offset = Offset(5);
        let aligned = offset.aligned_to(Alignment::new(8));
        assert_eq!(aligned.as_usize(), 8);
    }

    #[test]
    fn offset_add_size() {
        let offset = Offset(8);
        let new_offset = offset + Size::new(4);
        assert_eq!(new_offset.as_usize(), 12);
    }

    #[test]
    fn size_padding() {
        let size = Size::new(36);
        let padded = size.padded_to(Alignment::new(8));
        assert_eq!(padded.as_usize(), 40);
    }

    #[test]
    fn struct_layout_location() {
        let layouts = [
            Layout::new(8, 8), // id: i64
            Layout::new(8, 8), // lat: f64
            Layout::new(8, 8), // lng: f64
            Layout::new(8, 8), // rating: f64
            Layout::new(4, 4), // review_count: i32
            Layout::new(1, 1), // is_open: bool
        ];

        let struct_layout = StructLayout::from_layouts(layouts);

        let offsets: Vec<_> = struct_layout.offsets().map(|o| o.as_usize()).collect();
        assert_eq!(offsets, vec![0, 8, 16, 24, 32, 36]);
        assert_eq!(struct_layout.total_size().as_usize(), 40);
    }
}
