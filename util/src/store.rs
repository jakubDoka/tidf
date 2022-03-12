use std::{marker::PhantomData, ops::{Index, IndexMut, Deref, DerefMut}};

pub struct Table<A: Access + Invalid, T: Invalid> {
    lookup: Map<A>,
    data: Store<A, T>,
}

impl<A: Access + Invalid, T: Invalid> Table<A, T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            lookup: Map::with_capacity(capacity),
            data: Store::with_capacity(capacity),
        }
    }

    pub fn get(&self, key: &str) -> Option<&T> {
        self.get_by_id(Identifier::new(key))
    }

    pub fn get_by_id(&self, id: Identifier) -> Option<&T> {
        self.lookup.get_by_id(id).map(|&idx| &self.data[idx])
    }

    pub fn insert(&mut self, key: &str, value: T) {
        self.insert_by_id(Identifier::new(key), value);
    }

    pub fn insert_by_id(&mut self, id: Identifier, value: T) {
        let key = self.data.push(value);
        self.lookup.insert_by_id(id, key);
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }
}

impl<A: Access + Invalid, T: Invalid> Default for Table<A, T> {
    fn default() -> Self {
        Self {
            lookup: Map::default(),
            data: Store::default(),
        }
    }
}

impl<A: Access + Invalid, T: Invalid> Deref for Table<A, T> {
    type Target = Store<A, T>;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<A: Access + Invalid, T: Invalid> DerefMut for Table<A, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

pub struct Map<T: Invalid> {
    lookup: Vec<u32>,
    data: Vec<(Identifier, T, u32)>,
    free: u32,
}

impl<T: Invalid> Map<T> {
    pub fn new() -> Self {
        Self::default()     
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            lookup: vec![u32::MAX; Self::best_size(capacity)],
            data: Vec::with_capacity(capacity),
            free: u32::MAX,
        }
    }

    pub fn remove(&mut self, key: &str) -> Option<T> {
        self.remove_by_id(Identifier::new(key))
    }

    pub fn remove_by_id(&mut self, id: Identifier) -> Option<T> {
        let index = self.index_of(id);
        let mut current = self.lookup[index];
        let mut last_id = u32::MAX;
        while current != u32::MAX {
            let (identifier, value, next) = &mut self.data[current as usize];
            
            if *identifier == id && !value.is_invalid() {
                let saved_next = *next;
                *next = self.free as u32;
                let value = std::mem::replace(value, T::invalid());
                if last_id == u32::MAX {
                    self.lookup[index] = saved_next;
                } else {
                    self.data[last_id as usize].2 = saved_next;
                }
                self.free = current;
                return Some(value);
            }

            last_id = current;
            current = *next;
        }

        None
    }

    pub fn insert(&mut self, id: &str, t: T) -> Option<T> {
        self.insert_by_id(Identifier::new(id), t)
    }

    pub fn insert_by_id(&mut self, id: Identifier, t: T) -> Option<T> {
        let index = self.index_of(id);
        let mut current = self.lookup[index];

        let mut last_id = u32::MAX;

        while current != u32::MAX {
            let (identifier, data, next) = &mut self.data[current as usize];

            if data.is_invalid() {
                *identifier = id;
                *data = t;
                return None
            } else if id == *identifier {
                return Some(std::mem::replace(data, t))
            };

            last_id = current;
            current = *next;
        }

        let new = if self.free == u32::MAX {
            self.data.push((id, t, u32::MAX));
            self.data.len() as u32 - 1
        } else {
            let free = self.free;
            self.free = self.data[free as usize].2;
            self.data[free as usize] = (id, t, u32::MAX);
            free
        };

        if last_id == u32::MAX {
            self.lookup[index] = new;
        } else {
            self.data[last_id as usize].2 = new;
        }

        if self.data.len() > self.lookup.len() {
            self.expand();
        }

        None
    }

    #[cold]
    fn expand(&mut self) {
        let mut new = Self::with_capacity(self.data.len());

        for (id, t, _) in self.data.drain(..).filter(|(_, t, _)| !t.is_invalid()) {
            new.insert_by_id(id, t);
        }

        *self = new;
    }

    pub fn get(&self, name: &str) -> Option<&T> {
        self.get_by_id(Identifier::new(name))
    }

    pub fn get_by_id(&self, id: Identifier) -> Option<&T> {
        let index = self.index_of(id);
        let mut current = self.lookup[index as usize];

        while current != u32::MAX {
            let (ident, data, next) = &self.data[current as usize];
            if *ident == id && !data.is_invalid() {
                return Some(data);
            }
            current = *next;
        }   
        
        None
    }

    pub fn iter(&self) -> impl Iterator<Item = (Identifier, &T)> {
        self.data.iter().map(|(id, t, _)| (*id, t))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Identifier, &mut T)> {
        self.data.iter_mut().map(|(id, t, _)| (*id, t))
    }

    pub fn clear(&mut self) {
        self.lookup.iter_mut().for_each(|x| *x = u32::MAX);
        self.data.clear();
        self.free = u32::MAX;
    }

    fn index_of(&self, ident: Identifier) -> usize {
        ident.0 as usize & (self.lookup.len() - 1)
    }

    fn best_size(current: usize) -> usize {
        current.next_power_of_two()
    }
}

impl<T: Invalid> Default for Map<T> {
    fn default() -> Self {
        Self {
            lookup: vec![u32::MAX],
            data: Vec::new(),
            free: u32::MAX,
        }
    }
}

pub struct Store<A: Access, T> {
    data: Vec<T>,
    _pd: PhantomData<A>,
}

impl<A: Access, T> Store<A, T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, t: T) -> A {
        self.data.push(t);
        A::new(self.data.len() - 1)
    }

    pub fn iter(&self) -> impl Iterator<Item = (A, &T)> {
        self.data.iter().enumerate().map(|(i, t)| (A::new(i), t))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (A, &mut T)> {
        self.data.iter_mut().enumerate().map(|(i, t)| (A::new(i), t))
    }

    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.data.iter()
    }

    pub fn keys(&self) -> impl Iterator<Item = A> {
        (0..self.data.len()).map(A::new)
    }

    pub fn with_capacity(capacity: usize) -> Store<A, T> {
        Store {
            data: Vec::with_capacity(capacity),
            _pd: PhantomData,
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }
}

impl<A: Access, T> Default for Store<A, T> {
    fn default() -> Self {
        Self {
            data: Vec::new(),
            _pd: PhantomData,
        }
    }
}

impl<A: Access, T> Index<A> for Store<A, T> {
    type Output = T;

    fn index(&self, index: A) -> &Self::Output {
        &self.data[index.index()]
    }
}

impl<A: Access, T> IndexMut<A> for Store<A, T> {
    fn index_mut(&mut self, index: A) -> &mut Self::Output {
        &mut self.data[index.index()]
    }
}

pub struct Unverified<T: Invalid>(T);

impl<T: Invalid> Unverified<T> {
    pub fn valid(t: T) -> Self {
        Unverified(t)
    }

    pub fn invalid() -> Self {
        Unverified(T::invalid())
    }

    pub fn is_verified(&self) -> bool {
        !self.0.is_invalid()
    }

    pub fn is_invalid(&self) -> bool {
        self.0.is_invalid()
    }

    pub fn unwrap(self) -> T {
        self.0
    }

    pub fn into_option(self) -> Option<T> {
        self.into()
    }
}

impl<T: Invalid> Into<Option<T>> for Unverified<T> {
    fn into(self) -> Option<T> {
        if self.is_verified() {
            Some(self.unwrap())
        } else {
            None
        }
    }
}

impl<T: Invalid> From<Option<T>> for Unverified<T> {
    fn from(value: Option<T>) -> Self {
        if let Some(value) = value {
            Self(value)
        } else {
            Self::invalid()
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Identifier(pub u64);

impl Identifier {
    pub fn new(name: &str) -> Self {
        // This is sdbm hash.
        Self(
            name
                .as_bytes()
                .iter()
                .fold(0, |acc, &c| 
                    (c as u64)
                        .wrapping_add(acc << 6)
                        .wrapping_add(acc << 16)
                        .wrapping_sub(acc)
                )
        )
    }
}

impl Invalid for Identifier {
    fn invalid() -> Self {
        // empty string is invalid but it is fine for our purposes.
        Self(0)
    }

    fn is_invalid(&self) -> bool {
        self.0 == 0
    }
}

pub trait Invalid {
    fn invalid() -> Self;
    fn is_invalid(&self) -> bool;
}

pub trait Access: Clone + Copy + Eq + PartialEq {
    fn new(index: usize) -> Self;
    fn index(&self) -> usize;
}

#[macro_export]
macro_rules! create_access {
    ($($name:ident)*) => {
        $(
            #[derive(Copy, Clone, Debug, PartialEq, Eq)]
            pub struct $name(pub u32);
    
            impl $crate::store::Access for $name {
                fn new(index: usize) -> Self {
                    $name(index as u32)
                }
    
                fn index(&self) -> usize {
                    self.0 as usize
                }
            }
    
            impl $crate::store::Invalid for $name {
                fn invalid() -> Self {
                    $name(usize::MAX)
                }
    
                fn is_invalid(&self) -> bool {
                    self.0 == usize::MAX
                }
            }
    
            impl Default for $name {
                fn default() -> Self {
                    Self::invalid()
                }
            }
        )*
    };
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;

    use super::Invalid;

    impl Invalid for u32 {
        fn invalid() -> Self {
            u32::MAX
        }

        fn is_invalid(&self) -> bool {
            *self == u32::MAX
        }
    }

    #[test]
    fn fuzz_map() {
        use super::*;
        
        let mut data = vec![];
        let mut std_map = HashMap::new();
        let mut map = Map::new();
        let mut rng = ChaCha8Rng::seed_from_u64(2); //rand::thread_rng();

        data.extend((0..1000000).map(|i| {
            let len = 10; //rng.gen_range(1..10);
            let mut string = String::with_capacity(len);
            for _ in 0..len {
                string.push(rng.gen_range('a'..'z'));
            }
            (string, i)
        }));

        crate::bench("insert my", || {
            for (key, value) in data.iter() {
                map.insert(&key.clone(), *value);
            }
        });

        crate::bench("insert std", || {
            for (key, value) in data.iter() {
                std_map.insert(key.clone(), *value);
            }
        });

        crate::bench("lookup my", || {
            for (key, _) in data.iter() {
                map.get(&key);
            }
        });           

        crate::bench("lookup std", || {
            for (key, _) in data.iter() {
                std_map.get(key);
            }
        });

        crate::bench("remove my", || {
            for (key, _) in data.iter() {
                map.remove(&key);
            }
        });

        crate::bench("remove std", || {
            for (key, _) in data.iter() {
                std_map.remove(key);
            }
        });

        map.clear();
        std_map.clear();
    }
}