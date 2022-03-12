use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;

pub use derive::Bitwise;

pub struct Decoder {
    buffer: Vec<u8>,
    cursor: usize,
}

impl Decoder {
    pub fn new(buffer: Vec<u8>) -> Self {
        Self {
            buffer,
            cursor: 0,
        }
    }

    pub fn buffer(&mut self) -> &mut Vec<u8> {
        self.cursor = 0;
        &mut self.buffer
    }

    pub fn decode<T: Bitwise + Default>(&mut self) -> Option<T> {
        let mut t = T::default();
        t.decode(&mut self.cursor, &self.buffer)?;
        Some(t)
    }

    pub fn decode_into<T: Bitwise>(&mut self, target: &mut T) -> Option<()> {
        target.decode(&mut self.cursor, &self.buffer)
    }
}

pub struct BitwiseBoundCheck<T: Bitwise>(pub PhantomData<T>);

pub trait Bitwise {
    fn encode(&self, buffer: &mut Vec<u8>);
    fn decode(&mut self, cursor: &mut usize, buffer: &[u8]) -> Option<()>;
}

macro_rules! impl_bitwise_for_number {
    ($($number:ident)*) => {
        $(
            impl Bitwise for $number {
                fn encode(&self, buffer: &mut Vec<u8>) {
                    let bytes = self.to_le_bytes();
                    buffer.extend_from_slice(&bytes);
                }
            
                fn decode(&mut self, cursor: &mut usize, buffer: &[u8]) -> Option<()> {
                    if buffer.len() < *cursor + std::mem::size_of::<$number>() {
                        return None;
                    }
            
                    *self = $number::from_le_bytes(buffer[*cursor..*cursor + std::mem::size_of::<$number>()].try_into().unwrap());
                    *cursor += std::mem::size_of::<$number>();
            
                    Some(())
                }
            }
        )*
    };
}

impl Bitwise for String {
    fn encode(&self, buffer: &mut Vec<u8>) {
        self.len().encode(buffer);
        buffer.extend_from_slice(self.as_bytes());
    }

    fn decode(&mut self, cursor: &mut usize, buffer: &[u8]) -> Option<()> {
        let mut len = 0;
        usize::decode(&mut len, cursor, buffer)?;

        // prevents injected huge allocations that would crash a program
        if buffer.len() < *cursor + len {
            return None;
        }

        // we take invalid string as aggression and ignore it
        *self = std::str::from_utf8(&buffer[*cursor..*cursor + len]).ok()?.to_string();
        *cursor += len;

        Some(())
    }
}

impl<K: Bitwise + Default + Hash + Eq, V: Bitwise + Default> Bitwise for HashMap<K, V> {
    fn encode(&self, buffer: &mut Vec<u8>) {
        self.len().encode(buffer);
        // don't use tuple as ye don't care about alignment
        buffer.reserve(self.len() * (std::mem::size_of::<K>() + std::mem::size_of::<V>()));
        for (k, v) in self {
            k.encode(buffer);
            v.encode(buffer);
        }
    }

    fn decode(&mut self, cursor: &mut usize, buffer: &[u8]) -> Option<()> {
        let mut len = 0;
        usize::decode(&mut len, cursor, buffer)?;

        for _ in 0..len {
            let mut k = K::default();
            let mut v = V::default();
            k.decode(cursor, buffer)?;
            v.decode(cursor, buffer)?;
            self.insert(k, v);
        }

        Some(())
    }
}

impl<T: Bitwise + Default> Bitwise for Vec<T> {
    fn encode(&self, buffer: &mut Vec<u8>) {
        self.len().encode(buffer);
        buffer.reserve(self.len() * std::mem::size_of::<T>());
        for t in self {
            t.encode(buffer);
        }
    }

    fn decode(&mut self, cursor: &mut usize, buffer: &[u8]) -> Option<()> {
        let mut len = 0;
        usize::decode(&mut len, cursor, buffer)?;

        // prevents injected huge allocations that would crash a program
        if len > buffer.len() - *cursor {
            return None;
        }

        self.reserve(len);

        for _ in 0..len {
            let mut t = T::default();
            t.decode(cursor, buffer)?;
            self.push(t);
        }

        Some(())
    }
}

impl Bitwise for bool {
    fn encode(&self, buffer: &mut Vec<u8>) {
        buffer.push(*self as u8);
    }

    fn decode(&mut self, cursor: &mut usize, buffer: &[u8]) -> Option<()> {
        if buffer.len() < *cursor + 1 {
            return None;
        }

        *self = buffer[*cursor] != 0;
        *cursor += 1;

        Some(())
    }
}

impl_bitwise_for_number!(
    u8 u16 u32 u64 u128 usize
    i8 i16 i32 i64 i128 isize
    f32 f64
);

#[cfg(test)]
mod test {
    use super::*;

    #[derive(Bitwise, Debug, Default, PartialEq)]
    struct Foo {
        a: u8,
        b: u16,
        c: u32,
        d: u64,
        e: u128,
        f: usize,
        g: i8,
        h: i16,
        i: i32,
        j: i64,
        k: i128,
        l: isize,
        m: f32,
        n: f64,
        o: String,
        p: Vec<u8>,
        q: HashMap<u8, u16>,
        r: bool,
    }

    #[test]
    fn test_all() {
        let mut buffer = Vec::new();
        let mut cursor = 0;

        let foo = Foo {
            a: 1,
            b: 2,
            c: 3,
            d: 4,
            e: 5,
            f: 6,
            g: 7,
            h: 8,
            i: 9,
            j: 10,
            k: 11,
            l: 12,
            m: 13.0,
            n: 14.0,
            o: "hello world".to_string(),
            p: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20],
            q: {
                let mut q = HashMap::new();
                q.insert(1, 2);
                q.insert(3, 4);
                q.insert(5, 6);
                q
            },
            r: true,
        };

        foo.encode(&mut buffer);

        let mut foo2 = Foo::default();
        foo2.decode(&mut cursor, &buffer).unwrap();

        assert_eq!(foo, foo2);
    }

}