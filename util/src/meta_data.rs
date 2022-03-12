use std::collections::HashMap;

pub use minimal_yaml::{parse, Entry, Yaml};

#[macro_export]
macro_rules! impl_deserialize_state {
    ($id:ty) => {
        impl Deserialize for $id {
            fn deserialize<V, T: State<V>>(state: &mut T) -> Result<Self, String> {
                Ok(state.parse(str::deserialize(state)?)?)
            }
        }
    };
}

pub trait Deserialize<T>: Sized + Default {
    fn deserialize_into(&mut self, state: &mut T, node: Yaml) -> Result<(), String>;

    fn deserialize(state: &mut T, node: Yaml) -> Result<Self, String> {
        let mut result = Self::default();
        result.deserialize_into(state, node)?;
        Ok(result)
    }
}

pub fn extract_field<'a>(fields: &mut Vec<Entry<'a>>, name: &str) -> Option<Yaml<'a>> {
    fields
        .iter()
        .position(|field| field.key == Yaml::Scalar(name))
        .map(|index| fields.remove(index).value)
}

macro_rules! impl_deserialize_scalar {
    ($($t:ty),*) => {
        $(
            impl<T> Deserialize<T> for $t {
                fn deserialize_into(&mut self, _state: &mut T, node: Yaml) -> Result<(), String> {
                    match node {
                        Yaml::Scalar(s) => *self = s.parse::<$t>().map_err(|e| e.to_string())?,
                        _ => return Err(format!("expected scalar, got {:?}", node)),
                    }

                    Ok(())
                }
            }
        )*
    };
}

impl<T, E: Deserialize<T>> Deserialize<T> for Vec<E> {
    fn deserialize_into(&mut self, state: &mut T, node: Yaml) -> Result<(), String> {
        match node {
            Yaml::Sequence(seq) => {
                self.reserve(seq.len());
                for (i, item) in seq.into_iter().enumerate() {
                    self.push(
                        E::deserialize(state, item)
                            .map_err(|err| format!("at index {}: {}", i, err))?,
                    );
                }
                Ok(())
            }
            _ => Err(format!("expected sequence, got {:?}", node)),
        }
    }
}

impl<T, E: Deserialize<T>> Deserialize<T> for HashMap<String, E> {
    fn deserialize_into(&mut self, state: &mut T, node: Yaml) -> Result<(), String> {
        match node {
            Yaml::Mapping(map) => {
                self.reserve(map.len());
                for Entry { key, value } in map {
                    self.insert(
                        match key {
                            Yaml::Scalar(str) => str.to_string(),
                            _ => return Err(format!("expected scalar, got {:?}", key)),
                        },
                        E::deserialize(state, value)
                            .map_err(|err| format!("inside {}: {}", key, err))?,
                    );
                }
                Ok(())
            }
            _ => Err(format!("expected mapping, got {:?}", node)),
        }
    }
}

impl_deserialize_scalar!(
    i8, i16, i32, i64, isize, u8, u16, u32, u64, usize, f32, f64, bool, String, char
);
