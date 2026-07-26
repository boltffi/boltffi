use crate::core::{Error, Result};

const MAX_PARAMETER_SLOTS: u16 = 255;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SlotWidth {
    Single,
    Double,
}

pub trait Parameter {
    fn slot_width(&self) -> SlotWidth;
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Parameters<T>(Vec<T>);

impl SlotWidth {
    const fn slots(self) -> u16 {
        match self {
            Self::Single => 1,
            Self::Double => 2,
        }
    }
}

impl<T> Parameters<T>
where
    T: Parameter,
{
    pub fn for_static(parameters: Vec<T>) -> Result<Self> {
        let slots = parameters.iter().try_fold(0_u16, |slots, parameter| {
            slots.checked_add(parameter.slot_width().slots())
        });
        match slots.is_some_and(|slots| slots <= MAX_PARAMETER_SLOTS) {
            true => Ok(Self(parameters)),
            false => Err(Error::UnsupportedTarget {
                target: "jvm",
                shape: "method parameter slots exceed 255 units",
            }),
        }
    }

    pub fn as_slice(&self) -> &[T] {
        &self.0
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{Parameter, Parameters, SlotWidth};

    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    struct TestParameter(SlotWidth);

    impl Parameter for TestParameter {
        fn slot_width(&self) -> SlotWidth {
            self.0
        }
    }

    #[test]
    fn accepts_255_parameter_slots() {
        let parameters = std::iter::repeat_n(TestParameter(SlotWidth::Double), 127)
            .chain(std::iter::once(TestParameter(SlotWidth::Single)))
            .collect();

        assert!(Parameters::for_static(parameters).is_ok());
    }

    #[test]
    fn rejects_more_than_255_parameter_slots() {
        let parameters = std::iter::repeat_n(TestParameter(SlotWidth::Double), 128).collect();

        assert!(Parameters::for_static(parameters).is_err());
    }
}
